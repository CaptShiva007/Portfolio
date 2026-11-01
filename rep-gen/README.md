# Rep-Gen

Rep-Gen is a local Rust/Axum app for turning SOC ticket exports into an interactive report. CSV uploads are analyzed in memory and are not stored.

## Run locally

From this folder:

```powershell
cargo run
```

By default, Rep-Gen binds to `127.0.0.1:3000` so reports are not exposed to other machines on the network. To change the listener address, set `REP_GEN_BIND` (for example, `0.0.0.0:3000`) before starting it. The Docker image sets `REP_GEN_BIND=0.0.0.0:3000` so the container port can be published.

Open <http://127.0.0.1:3000>.

## Freshservice connection

The Freshservice integration is read-only and connects to `ampcuscyber.freshservice.com`. It reads `FRESHSERVICE_API_KEY` from the environment of the Rep-Gen server process; the key is not sent to the browser, stored in the report, written to disk, or logged by the app. If the variable is not configured, CSV reporting remains available and the Freshservice sync button is disabled.

To start the server with the key entered without putting it in shell history or a file, run these commands in the same PowerShell window:

```powershell
$secret = Read-Host "Freshservice API key" -AsSecureString
$env:FRESHSERVICE_API_KEY = [System.Net.NetworkCredential]::new("", $secret).Password
cargo run
```

Use **Sync Freshservice tickets** in the app to retrieve all pages returned by the Freshservice tickets API. The integration sends read-only `GET /api/v2/tickets` requests, with 100 tickets per page. Freshservice API-key permissions and rate limits still apply.

Freshservice numeric priorities are mapped to severity as 1=Low, 2=Medium, 3=High, and 4=Critical. Ticket status values 2–5 are displayed as Open, Pending, Resolved, and Closed. The **Alerts raised** metric counts tickets whose custom field name includes `annotation stage` and whose value is `New Incident Created`, provided a custom field name includes `email sent` and its value contains a timestamp/value. If either field is absent, the report marks this metric unavailable and includes a data-quality note.
