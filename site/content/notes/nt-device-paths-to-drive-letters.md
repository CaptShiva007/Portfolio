---
title: Turning NT device paths back into drive letters
date: 2026-11-02
tags: [windows, etw, rust, paths]
summary: Kernel ETW gives you \Device\HarddiskVolume3\... paths. QueryDosDeviceW maps them back to C:\... so you can actually open the file.
---

Kernel-level Windows sources, like the `Microsoft-Windows-Kernel-Process` ETW provider, report file paths in NT device form:

```
\Device\HarddiskVolume3\Windows\System32\cmd.exe
```

Win32 file APIs (and Rust's `std::fs`) can't open that. You need `C:\Windows\System32\cmd.exe`. I hit this in [Positron](/projects/positron), where the YARA scanner has to read the image of every new process.

## The fix

`QueryDosDeviceW("C:")` returns the device behind a drive letter, e.g. `\Device\HarddiskVolume3`. Build the table once for `A:` through `Z:`, then swap the prefix:

```rust
for (device, drive) in map {
    let Some(head) = path.get(..device.len()) else { continue };
    let rest = &path[device.len()..];
    if head.eq_ignore_ascii_case(device) && (rest.is_empty() || rest.starts_with('\\')) {
        return format!("{drive}{rest}");
    }
}
path.to_string()
```

## Gotchas

- **Check for a separator after the prefix.** Otherwise `\Device\HarddiskVolume1` also matches `\Device\HarddiskVolume10\...` and you get the wrong drive.
- **Compare case-insensitively.** NT paths aren't guaranteed to come back in one casing.
- **`QueryDosDeviceW` returns a list.** The buffer holds several NUL-terminated strings; the first one is the active mapping.
- **Pass unknown paths through unchanged.** Network paths show up as `\Device\Mup\...`, and drives mounted after startup won't be in the table. A raw path is better than a wrong one.

## Quick check from PowerShell

There's no built-in cmdlet for this, but `fltmc volumes` lists each volume's device name next to its drive letter:

```powershell
fltmc volumes
```

(Needs an elevated shell.)
