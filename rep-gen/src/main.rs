use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, net::SocketAddr, time::Duration};

const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const FRESHSERVICE_HOST: &str = "ampcuscyber.freshservice.com";
const FRESHSERVICE_PAGE_SIZE: usize = 100;

#[derive(Serialize)]
struct CountItem { label: String, count: usize }

#[derive(Serialize)]
struct TicketRow {
    id: String,
    title: String,
    severity: String,
    status: String,
    created: String,
    assignee: String,
}

#[derive(Serialize)]
struct Report {
    total: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    open: usize,
    closed: usize,
    acknowledged: usize,
    sla_met: Option<usize>,
    sla_breached: Option<usize>,
    sla_total: Option<usize>,
    period_start: String,
    period_end: String,
    severity: Vec<CountItem>,
    statuses: Vec<CountItem>,
    trend: Vec<CountItem>,
    top_alerts: Vec<CountItem>,
    rows: Vec<TicketRow>,
    columns: Vec<String>,
    notes: Vec<String>,
    alerts_raised: Option<usize>,
}

#[derive(Deserialize)]
struct FreshserviceTickets {
    tickets: Vec<Value>,
}

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('_', "").replace('-', "").replace(' ', "")
}

fn find_col(headers: &[String], candidates: &[&str]) -> Option<usize> {
    let normalized: Vec<String> = headers.iter().map(|h| norm(h)).collect();
    for candidate in candidates {
        let needle = norm(candidate);
        if let Some(i) = normalized.iter().position(|h| h == &needle) { return Some(i); }
    }
    for candidate in candidates {
        let needle = norm(candidate);
        if let Some(i) = normalized.iter().position(|h| h.contains(&needle)) { return Some(i); }
    }
    None
}

fn value<'a>(row: &'a [String], idx: Option<usize>) -> &'a str {
    idx.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("")
}

fn classify_severity(s: &str) -> &'static str {
    let s = s.to_ascii_lowercase();
    if s.contains("critical") || s.contains("urgent") || s == "p1" || s == "4" || s.contains("sev1") { "Critical" }
    else if s.contains("high") || s == "p2" || s == "3" || s.contains("sev2") { "High" }
    else if s.contains("medium") || s.contains("moderate") || s == "p3" || s == "2" || s.contains("sev3") { "Medium" }
    else if s.contains("low") || s == "p4" || s == "1" || s.contains("sev4") || s.contains("informational") || s == "info" { "Low" }
    else { "Unclassified" }
}

fn bucket_counts(map: HashMap<String, usize>) -> Vec<CountItem> {
    let mut out: Vec<CountItem> = map.into_iter().map(|(label, count)| CountItem { label, count }).collect();
    out.sort_by(|a,b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    out
}

fn date_part(s: &str) -> String {
    let t = s.trim();
    if t.get(4..5) == Some("-") && t.get(7..8) == Some("-") {
        t.get(..10).unwrap_or(t).to_string()
    } else { t.to_string() }
}

fn parse_report(bytes: &[u8]) -> Result<Report, String> {
    if bytes.is_empty() { return Err("The uploaded CSV is empty.".into()); }
    if bytes.len() > MAX_UPLOAD_BYTES { return Err("CSV exceeds the 25 MB upload limit.".into()); }

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers: Vec<String> = reader.headers()
        .map_err(|e| format!("Could not read CSV headers: {e}"))?
        .iter().map(|s| s.trim().trim_start_matches('\u{feff}').to_string()).collect();
    if headers.is_empty() { return Err("No header row was found in the CSV.".into()); }

    let id_col = find_col(&headers, &["ticket id", "case id", "alert id", "id", "number", "ticket"]);
    let title_col = find_col(&headers, &["alert name", "rule name", "title", "name", "subject", "use case", "offense description"]);
    let severity_col = find_col(&headers, &["severity", "priority", "risk level", "criticality"]);
    let status_col = find_col(&headers, &["status", "state", "ticket status", "case status"]);
    let created_col = find_col(&headers, &["created at", "created date", "creation time", "created", "start time", "timestamp", "opened"]);
    let assignee_col = find_col(&headers, &["assigned to", "assignee", "responder id", "owner", "analyst", "assigned"]);
    let sla_col = find_col(&headers, &["sla status", "sla result", "sla met", "within sla", "sla"]);
    let desc_col = find_col(&headers, &["description", "summary", "details", "alert description", "message"]);
    let annotation_col = find_col(&headers, &["annotation stage"]);
    let email_sent_col = find_col(&headers, &["email sent"]);

    let mut total = 0usize;
    let mut severity_counts: HashMap<String, usize> = HashMap::new();
    let mut status_counts: HashMap<String, usize> = HashMap::new();
    let mut trend_counts: HashMap<String, usize> = HashMap::new();
    let mut alert_counts: HashMap<String, usize> = HashMap::new();
    let mut rows: Vec<TicketRow> = Vec::new();
    let mut earliest = String::new();
    let mut latest = String::new();
    let mut sla_met = 0usize;
    let mut sla_breached = 0usize;
    let mut sla_seen = false;
    let mut alerts_raised = 0usize;

    for record in reader.records() {
        let record = record.map_err(|e| format!("CSV row could not be parsed: {e}"))?;
        let row: Vec<String> = record.iter().map(str::to_string).collect();
        if row.iter().all(|v| v.trim().is_empty()) { continue; }
        total += 1;

        let raw_sev = value(&row, severity_col);
        let sev = classify_severity(raw_sev).to_string();
        *severity_counts.entry(sev.clone()).or_insert(0) += 1;

        let status_raw = value(&row, status_col);
        let status = if status_raw.is_empty() { "Unspecified".to_string() } else { status_raw.to_string() };
        *status_counts.entry(status.clone()).or_insert(0) += 1;

        if let (Some(annotation_idx), Some(email_sent_idx)) = (annotation_col, email_sent_col) {
            let annotation = norm(value(&row, Some(annotation_idx)));
            if annotation.contains("newincidentcreated") && email_was_sent(value(&row, Some(email_sent_idx))) {
                alerts_raised += 1;
            }
        }

        let created = value(&row, created_col).to_string();
        if !created.is_empty() {
            let d = date_part(&created);
            *trend_counts.entry(d.clone()).or_insert(0) += 1;
            if earliest.is_empty() || d < earliest { earliest = d.clone(); }
            if latest.is_empty() || d > latest { latest = d; }
        }

        let title = {
            let t = value(&row, title_col);
            if !t.is_empty() { t.to_string() }
            else {
                let d = value(&row, desc_col);
                if !d.is_empty() { d.chars().take(110).collect() }
                else { "Untitled alert".to_string() }
            }
        };
        *alert_counts.entry(title.clone()).or_insert(0) += 1;

        if let Some(i) = sla_col {
            let s = row.get(i).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
            if !s.trim().is_empty() {
                sla_seen = true;
                if s.contains("breach") || s.contains("miss") || s == "no" || s == "false" || s.contains("violation") {
                    sla_breached += 1;
                } else if s.contains("met") || s.contains("within") || s == "yes" || s == "true" || s.contains("compliant") {
                    sla_met += 1;
                }
            }
        }

        let id = value(&row, id_col);
        let assignee = value(&row, assignee_col);
        rows.push(TicketRow {
            id: if id.is_empty() { format!("ROW-{total}") } else { id.to_string() },
            title,
            severity: sev,
            status,
            created,
            assignee: if assignee.is_empty() { "Unassigned".into() } else { assignee.into() },
        });
    }

    if total == 0 { return Err("The CSV contains headers but no data rows.".into()); }

    rows.sort_by(|a,b| b.created.cmp(&a.created));
    rows.truncate(100);

    let mut notes = Vec::new();
    if severity_col.is_none() { notes.push("No recognizable severity/priority column was found; severity metrics may be incomplete.".into()); }
    if status_col.is_none() { notes.push("No recognizable status column was found.".into()); }
    if created_col.is_none() { notes.push("No recognizable creation timestamp was found; the trend chart and period are unavailable.".into()); }
    if title_col.is_none() { notes.push("No recognizable alert/rule title column was found; titles are inferred from descriptions where possible.".into()); }
    if !sla_seen { notes.push("No explicit SLA result values were detected. SLA compliance is not inferred from timestamps.".into()); }
    if annotation_col.is_none() || email_sent_col.is_none() {
        notes.push("The alerts-raised metric requires annotation-stage and email-sent fields.".into());
    }

    let count = |key: &str| severity_counts.get(key).copied().unwrap_or(0);
    let open = status_counts.iter().filter(|(k,_)| {
        let s = k.to_ascii_lowercase();
        !(s.contains("closed") || s.contains("resolved") || s.contains("false positive") || s.contains("rejected"))
    }).map(|(_,v)| *v).sum();
    let closed = status_counts.iter().filter(|(k,_)| {
        let s = k.to_ascii_lowercase();
        s.contains("closed") || s.contains("resolved")
    }).map(|(_,v)| *v).sum();
    let acknowledged = status_counts.iter().filter(|(k,_)| k.to_ascii_lowercase().contains("acknowledged")).map(|(_,v)| *v).sum();

    let mut trend = bucket_counts(trend_counts);
    // Chart should be chronological, while top lists remain descending.
    trend.sort_by(|a,b| a.label.cmp(&b.label));
    if trend.len() > 31 { trend = trend.split_off(trend.len() - 31); }

    Ok(Report {
        total, critical: count("Critical"), high: count("High"), medium: count("Medium"), low: count("Low"),
        open, closed, acknowledged,
        sla_met: if sla_seen { Some(sla_met) } else { None },
        sla_breached: if sla_seen { Some(sla_breached) } else { None },
        sla_total: if sla_seen { Some(sla_met + sla_breached) } else { None },
        period_start: earliest, period_end: latest,
        severity: bucket_counts(severity_counts),
        statuses: bucket_counts(status_counts),
        trend,
        top_alerts: bucket_counts(alert_counts).into_iter().take(8).collect(),
        rows, columns: headers, notes,
        alerts_raised: if annotation_col.is_some() && email_sent_col.is_some() { Some(alerts_raised) } else { None },
    })
}

fn freshservice_status_name(status: &Value) -> String {
    match status.as_i64() {
        Some(2) => "Open".into(),
        Some(3) => "Pending".into(),
        Some(4) => "Resolved".into(),
        Some(5) => "Closed".into(),
        _ => status.as_str().unwrap_or("Unspecified").to_string(),
    }
}

fn freshservice_to_csv(tickets: &[Value]) -> Result<Vec<u8>, String> {
    let mut custom_columns = Vec::new();
    for ticket in tickets {
        if let Some(fields) = ticket.get("custom_fields").and_then(Value::as_object) {
            for key in fields.keys() {
                if !custom_columns.contains(key) {
                    custom_columns.push(key.clone());
                }
            }
        }
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    let mut headers = vec![
        "id".to_string(),
        "subject".to_string(),
        "priority".to_string(),
        "status".to_string(),
        "created_at".to_string(),
        "responder_id".to_string(),
    ];
    headers.extend(custom_columns.iter().cloned());
    writer.write_record(&headers).map_err(|e| format!("Could not prepare Freshservice report: {e}"))?;

    for ticket in tickets {
        let fields = ticket.get("custom_fields").and_then(Value::as_object);
        let mut row = vec![
            ticket.get("id").map(value_to_string).unwrap_or_default(),
            ticket.get("subject").map(value_to_string).unwrap_or_default(),
            ticket.get("priority").map(value_to_string).unwrap_or_default(),
            freshservice_status_name(ticket.get("status").unwrap_or(&Value::Null)),
            ticket.get("created_at").map(value_to_string).unwrap_or_default(),
            ticket.get("responder_id").map(value_to_string).unwrap_or_default(),
        ];
        row.extend(custom_columns.iter().map(|key| {
            fields.and_then(|object| object.get(key)).map(value_to_string).unwrap_or_default()
        }));
        writer.write_record(&row).map_err(|e| format!("Could not prepare Freshservice report: {e}"))?;
    }

    writer.into_inner().map_err(|e| format!("Could not prepare Freshservice report: {}", e.error()))
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn email_was_sent(value: &str) -> bool {
    !matches!(norm(value).as_str(), "" | "false" | "no" | "notsent" | "notset" | "null" | "0")
}

async fn fetch_freshservice_tickets(api_key: &str) -> Result<Vec<Value>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Could not initialize the Freshservice connection.".to_string())?;
    let mut tickets = Vec::new();
    let mut page = 1usize;

    loop {
        let response = client
            .get(format!("https://{FRESHSERVICE_HOST}/api/v2/tickets"))
            .query(&[("per_page", FRESHSERVICE_PAGE_SIZE), ("page", page)])
            .basic_auth(api_key, Some("X"))
            .send()
            .await
            .map_err(|_| "Could not connect to Freshservice. Check network access and try again.".to_string())?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Freshservice rejected the API key. Check FRESHSERVICE_API_KEY and try again.".into());
        }
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err("Freshservice denied access. Check the API key permissions.".into());
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("Freshservice rate limit reached. Wait a moment, then try syncing again.".into());
        }
        if !response.status().is_success() {
            return Err(format!("Freshservice returned HTTP {} while listing tickets.", response.status().as_u16()));
        }

        let batch = response.json::<FreshserviceTickets>().await
            .map_err(|_| "Freshservice returned an invalid ticket response.".to_string())?
            .tickets;
        let batch_len = batch.len();
        tickets.extend(batch);
        if batch_len < FRESHSERVICE_PAGE_SIZE {
            break;
        }
        page = page.checked_add(1).ok_or_else(|| "Freshservice pagination limit was exceeded.".to_string())?;
    }

    if tickets.is_empty() {
        return Err("No tickets were returned by Freshservice.".into());
    }
    Ok(tickets)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn health() -> impl IntoResponse { (StatusCode::OK, "ok") }

async fn analyze(mut multipart: Multipart) -> Response {
    let mut csv_bytes: Option<Vec<u8>> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => return (StatusCode::BAD_REQUEST, "Could not read uploaded form data.").into_response(),
        };
        if field.name() == Some("file") {
            match field.bytes().await {
                Ok(bytes) => {
                    if bytes.len() > MAX_UPLOAD_BYTES {
                        return (StatusCode::PAYLOAD_TOO_LARGE, "CSV exceeds the 25 MB upload limit.").into_response();
                    }
                    csv_bytes = Some(bytes.to_vec());
                    break;
                }

                Err(_) => return (StatusCode::BAD_REQUEST, "Could not read uploaded file.").into_response(),
            }
        }
    }

    let Some(bytes) = csv_bytes else {
        return (StatusCode::BAD_REQUEST, "Choose a CSV file to analyze.").into_response();
    };

    match parse_report(&bytes) {
        Ok(report) => {
            let mut response = Json(report).into_response();
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-cache, must-revalidate, private"));
            response.headers_mut().insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }
        Err(message) => (StatusCode::UNPROCESSABLE_ENTITY, message).into_response(),
    }
}

async fn freshservice_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "configured": std::env::var("FRESHSERVICE_API_KEY").map(|key| !key.trim().is_empty()).unwrap_or(false),
        "host": FRESHSERVICE_HOST
    }))
}

async fn freshservice_report() -> Response {
    let api_key = match std::env::var("FRESHSERVICE_API_KEY") {
        Ok(key) if !key.trim().is_empty() => key,
        _ => return (StatusCode::SERVICE_UNAVAILABLE, "Set FRESHSERVICE_API_KEY in the environment where rep-gen is launched, then restart the server.").into_response(),
    };

    let tickets = match fetch_freshservice_tickets(&api_key).await {
        Ok(tickets) => tickets,
        Err(message) => return (StatusCode::BAD_GATEWAY, message).into_response(),
    };
    let bytes = match freshservice_to_csv(&tickets) {
        Ok(bytes) => bytes,
        Err(message) => return (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
    };
    match parse_report(&bytes) {
        Ok(mut report) => {
            if report.alerts_raised.is_none() {
                report.notes.push("Freshservice tickets did not include recognizable annotation-stage and email-sent custom fields; the alerts-raised metric is unavailable.".into());
            }
            let mut response = Json(report).into_response();
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-cache, must-revalidate, private"));
            response.headers_mut().insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }
        Err(message) => (StatusCode::UNPROCESSABLE_ENTITY, message).into_response(),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(health))
        .route("/api/report", post(analyze))
        .route("/api/freshservice/status", get(freshservice_status))
        .route("/api/freshservice/report", post(freshservice_report))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 1024 * 1024));

    let addr = std::env::var("REP_GEN_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse::<SocketAddr>()
        .expect("REP_GEN_BIND must be a valid socket address");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind Rep-Gen listener");
    println!("rep-gen listening on http://{addr}");
    axum::serve(listener, app).await.expect("server failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_raised_requires_stage_and_email_timestamp() {
        let csv = b"id,subject,priority,status,created_at,cf_annotation_stage,cf_email_sent\n\
1,Endpoint alert,4,2,2026-10-15T10:00:00Z,New Incident Created,2026-10-15T10:05:00Z\n\
2,Phishing alert,3,4,2026-10-15T11:00:00Z,New Incident Created,\n\
3,Login alert,2,2,2026-10-15T12:00:00Z,In Review,2026-10-15T12:05:00Z\n\
4,Repeated alert,1,2,2026-10-15T13:00:00Z,New Incident Created,false\n";
        let report = parse_report(csv).unwrap();
        assert_eq!(report.alerts_raised, Some(1));
        assert_eq!(report.critical, 1);
        assert_eq!(report.high, 1);
        assert_eq!(report.medium, 1);
        assert_eq!(report.low, 1);
    }

    #[test]
    fn freshservice_custom_fields_are_included_in_report() {
        let ticket = serde_json::json!({
            "id": 42,
            "subject": "Suspicious login",
            "priority": 4,
            "status": 2,
            "created_at": "2026-10-15T10:00:00Z",
            "responder_id": 123,
            "custom_fields": {
                "cf_annotation_stage": "New Incident Created",
                "cf_email_sent": "2026-10-15T10:05:00Z"
            }
        });
        let bytes = freshservice_to_csv(&[ticket]).unwrap();
        let report = parse_report(&bytes).unwrap();
        assert_eq!(report.alerts_raised, Some(1));
        assert_eq!(report.total, 1);
        assert_eq!(report.critical, 1);
        assert_eq!(report.rows[0].assignee, "123");
    }
}
