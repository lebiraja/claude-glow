# G614JU Aura HID protocol (verified on hardware)

Device: `/dev/hidraw0` — ASUS N-KEY `0b05:19b6`, world-writable, 64-byte output reports, report ID `0x5d`.

## What works
**Zoned direct frame, streamed.** Resend every ~50 ms; the firmware reverts to the saved mode if frames stop.

| Byte(s) | Meaning |
|---|---|
| 0..1 | `5d bc` |
| 2,3,4 | `01 01 04` (zoned, multi-zone) |
| 9,12,15,18 | RGB of keyboard zones 1..4, left → right |
| 27,30,33,36,39,42 | RGB of the 6 lightbar LEDs (right side → left side, offsets in the order tested B1..B6) |

Send `5d bc` (init) once before the first frame.

## What does not work
- **Per-key groups** (`5d bc 00 01 01 01 <grp<<4> 10 …`): every packet lights the whole keyboard; last write wins.
- **`asusctl aura effect --zone lightbar-left`**: rejected by asusd (`NotSupported`).
- **Frames followed by `5d b5` / `5d b4` (commit)**: firmware snaps back to the saved built-in mode.
- **Built-in static packet zone byte** (`5d b3 <zone> 00 r g b …` + commit): zones 1..4 = keyboard quarters, with the left bar tied to zone 1 and the right bar tied to zone 4. Zones 5..7 did not isolate logo or bar.

## Restore
`asusctl aura effect static -c 414141` then set `Brightness` back to 1 via D-Bus (`xyz.ljones.Aura`).

## Notes
- Bar LED names `bar1..bar6` follow packet offset order (27, 30, … 42). Physical positions were checked on hardware.
- Per-key keyboard addressing and the logo zone are not mapped. They are not needed for status lighting.
- Raw writes go to `/dev/hidraw0` as the user; avoid the `0x5a` feature reports (firmware territory).
