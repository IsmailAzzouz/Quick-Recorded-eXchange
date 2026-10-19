# Browser exchange workspace

Mode: Operate. Scope: web/app. User-pinned, code-led refinement.

Confirmed user direction: black and white with easily reversible dark/light mode;
Swiss/minimalist, efficient, direct, low stress; only Encode or Receive visible at
once. No new UI framework or downloaded fonts. All file processing remains local.

FIRST VIEWPORT: QRX identity and two small preference switches, a clear two-tab
mode selector, one action heading. Desktop left rail contains file selection,
closed-by-default settings, and export; the right contains the QR artifact with
playback controls. The UI uses an asymmetric grid, native Helvetica/Arial,
neutral square controls, whitespace, and thin rules. Mobile stacks these in order.

Signature interaction: switching Encode/Receive replaces the visible workspace
while preserving each workspace's in-tab state; keyboard arrow/Home/End selection.
The theme switch reverses the interface immediately without changing QR pixels.
Motion grammar: only a 160ms switch thumb transition, disabled for reduced motion.

Functional acceptance: saved video contains actual QR pixels and recovers exact
original bytes; no reliance on live canvas capture; fixed video timestamps survive
suspension; cancel remains responsive; automatic download has a persistent manual
Save video alternative. Screen keep-awake and cached WASM remain functional.

Detector findings: flat-type-hierarchy is a source-import false positive; real
computed h1=36px, h2=32px, body=16px. Helvetica warning is an intentional exception:
Swiss design is user-pinned and local system fonts avoid network/font cost.
