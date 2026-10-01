# Device profiles

A profile describes one laptop's Aura controller. The bundled one is
[`crates/aura-hid/profiles/g614ju.toml`](../crates/aura-hid/profiles/g614ju.toml).

```toml
name = "ASUS ROG Strix G614JU"
vid = 0x0b05                      # USB vendor id of the N-KEY device
pid = 0x19b6                      # USB product id
keyboard_default = "#ffffff"      # colour sent to the "keys" group while streaming

restore = [                       # packets that return the saved built-in mode
  [0x5d, 0xb3, 0x00, 0x00, 0x41, 0x41, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
  [0x5d, 0xb5],
  [0x5d, 0xb4],
]

[packet]
len = 64
init = [0x5d, 0xbc]               # sent once before streaming
header = [0x5d, 0xbc, 0x01, 0x01, 0x04]   # first bytes of every frame

[[leds]]
name = "bar1"                     # unique name
group = "bar"                     # frames address groups ("bar", "keys")
offset = 27                       # byte offset of the R,G,B triple in the packet
```

Rules enforced at load time: LED names are unique, every `offset + 3` fits in `len`, and the header fits in `len`.

## Trying another laptop
1. Find the device: `lsusb | grep -i asus` and look for the "N-KEY Device" (`0b05:xxxx`).
2. Copy `g614ju.toml`, change `vid`/`pid`, and use [protocol.md](protocol.md) as a starting point
   (layouts differ per model; `asusctl`'s `aura_support.ron` lists per-model LED layouts).
3. Walk the offsets one at a time with `claude-glow probe <led>` to learn which byte drives which LED.
4. Load it by replacing the bundled profile (`Profile::g614ju()` in `crates/claude-glow/src/main.rs`) with `Profile::from_toml`.

Profile selection by CLI flag or DMI auto-detection is not implemented yet. Pull requests are welcome.
