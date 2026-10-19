---
name: Quick Recorded eXchange
description: A reversible monochrome workspace for local optical file exchange.
colors:
  background: "#fff"
  foreground: "#111"
  muted: "#626262"
  line: "#c5c5c5"
  surface: "#f4f4f4"
  dark-muted: "#b0b0b0"
  dark-surface: "#1d1d1d"
  qr-ink: "#000"
typography:
  display:
    fontFamily: "Helvetica, Arial, sans-serif"
    fontSize: "36px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-.03em"
  headline:
    fontFamily: "Helvetica, Arial, sans-serif"
    fontSize: "32px"
    fontWeight: 500
    lineHeight: 1.2
    letterSpacing: "-.025em"
  body:
    fontFamily: "Helvetica, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Helvetica, Arial, sans-serif"
    fontSize: "13px"
    fontWeight: 500
    lineHeight: 1.5
  supporting:
    fontFamily: "Helvetica, Arial, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.65
rounded:
  square: "0"
  switch: "12px"
spacing:
  tight: "8px"
  control: "10px"
  field: "16px"
  group: "20px"
  compact: "24px"
  section: "32px"
  column: "64px"
components:
  button-primary:
    backgroundColor: "{colors.foreground}"
    textColor: "{colors.background}"
    typography: "{typography.label}"
    rounded: "{rounded.square}"
    padding: "10px 16px"
  button-secondary:
    backgroundColor: "{colors.background}"
    textColor: "{colors.foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.square}"
    padding: "10px 16px"
  button-text:
    textColor: "{colors.muted}"
    padding: "2px 0"
  input:
    backgroundColor: "{colors.background}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.square}"
    padding: "8px 10px"
  preview:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.square}"
    width: "100%"
---

# Design System: Quick Recorded eXchange

## Overview

**Creative North Star: "The Swiss exchange workspace"**

A Swiss, minimalist workspace for optical file exchange. Black, white, neutral grays, local Helvetica, whitespace, and thin rules keep the file and its QR artifact central. The interface reverses cleanly between light and dark without changing the transfer pixels.

One workspace is visible at a time. Controls occupy a compact left rail; the output receives the larger right region. State remains in the tab when the user switches between Encode and Receive.

**Key Characteristics:**

- Reversible monochrome interface; invariant black-on-white QR output.
- One visible task, retained in-tab work, direct status and recovery actions.
- Asymmetric workspace, square controls, thin dividers, no decorative depth.
- Local system typography and no external fonts or UI framework.

## Colors

### Primary

Foreground is the action color: filled primary controls, active navigation, focus outlines, and error borders use the current foreground role. There is no chromatic accent.

### Neutral

Background and foreground reverse between themes. Light mode uses the muted, line, and surface tokens; dark mode uses dark-muted for secondary text, muted for rules, and dark-surface for empty previews. The dark foreground is background; the dark background is foreground. Error states use weight, border, and explicit language rather than a separate hue.

**The Transfer Pixels Rule.** Theme changes reverse interface roles only. QR modules remain pure black on white.

## Typography

**Display and Body Font:** Helvetica, Arial, sans-serif. The local system stack is a deliberate user-confirmed Swiss design and efficiency exception to the general Impeccable display-font preference.

The compact bold identity contrasts with a lighter task heading. The frontmatter defines the main roles; supporting text also includes task descriptions (14px), tab labels (17px; selected weight 600), empty-preview titles (20px), and footer/wake statuses (11px). Transfer values use tabular numerals. Descriptions and hints are bounded to 65ch where applicable.

At the mobile breakpoint, task headings reduce to 28px; the identity remains 36px. Preserve this hierarchy rather than adding oversized marketing typography.

## Layout

The centered shell has a maximum width of 1160px and horizontal padding of 40px. Header, tab row, task heading, workbench, and footer form one vertical sequence. The desktop workbench pairs a 250–320px control rail with flexible output and a 64px gap. Control groups use 20px spacing; settings use 16px.

At 850px and below, shell padding becomes 28px, the rail becomes 220–280px, and the workbench gap becomes 32px. At 650px and below, shell padding becomes 20px, the header wraps, the workbench stacks, output fills the width, settings form two columns with the final field spanning both, and the footer stacks. Ordinary buttons and fields increase from a minimum 42px to 44px height.

Desktop output is right-aligned and capped by `min(100%, max(300px, calc(100svh - 440px)))` to keep the controls reachable. Outgoing QR preview is square, with a 720 × 720 backing canvas; incoming video is 4:3 and contained without cropping. Long filenames and status messages wrap.

## Elevation & Depth

There are no shadows. Empty previews use a neutral surface; rules distinguish navigation, settings, completed exports, received files, and footer. The output is a working artifact, not a raised card.

**The Flat Surface Rule.** Use whitespace, thin rules, and surface tone to distinguish regions. Do not introduce shadows.

## Shapes

Controls, previews, and content regions are square. Borders are thin (1px); the active tab underline is 2px. The small preference switch is the intentional rounded exception, with a pill track and circular thumb. Progress is a flat 4px line.

## Components

### Buttons

Primary buttons invert foreground/background; secondary buttons use an outlined surface. Both share compact padding and square corners. Primary hover returns to an outlined appearance; secondary hover uses the subtle surface and foreground border. Pressed ordinary buttons invert. Disabled buttons use opacity .4 and a blocked cursor. Text actions are small, underlined, borderless controls. Icon-only frame controls carry accessible names and inline SVG.

### Inputs / Fields

Use native file, number, and select controls. Number/select fields have a thin line border, square shape, tabular values, and a minimum 42px height. The file selector retains a foreground outline. Global keyboard focus is a 2px foreground outline with 4px offset. Encoding settings begin collapsed in a native disclosure; FPS help explicitly explains the export cap.

### Navigation

Encode and Receive are semantic tabs with a thin baseline. Selection adds a foreground underline, foreground text, and increased weight. Exactly one panel is visible, while both remain mounted. Arrow keys cycle; Home/End select the first/last tab; roving tab stops follow selection. Switching modes preserves work, including an active export.

### Preference switches

Dark mode and Keep screen on use native checkbox semantics with switch roles. The theme follows the device until explicitly selected, then persists when storage is available. The thumb alone transitions (160ms, cubic-bezier(.22,1,.36,1)); reduced motion removes the transition. Forced colors restores native checkbox appearance. Keep-awake feedback and Retry remain adjacent to their switch.

### Transfer previews and results

An empty preview centers a short title and muted instruction. The canvas/video remains hidden until ready. Export disables sender settings and playback, reveals Cancel export, and reports the QR frame being written. Completion exposes persistent Save video alongside the automatic download attempt. Cancellation and errors are textual states; errors gain a foreground outline and stronger weight.

Received files are flat, ruled articles: filename, packet/verification status, progress, and a download action only after SHA-256 verification. Stop remains the explicit capture control. Keep controls and status useful across long operations; there are no artificial processing deadlines. These states describe the shipped interface, not device compatibility guarantees.

## Do's and Don'ts

### Do:

- **Do** preserve both workspace DOM trees and their in-tab state when switching modes.
- **Do** keep text status, visible focus, native field semantics, and explicit cancel/retry/save actions.
- **Do** use the existing local Helvetica/Arial stack and tabular numerals for transfer values.
- **Do** describe export as using up to 10 FPS; the playback input can accept 1–30 FPS.

### Don't:

- **Don't** reintroduce the rejected green palette, decorative cards, or downloaded fonts.
- **Don't** invert QR pixels in dark mode or add decorative motion.
- **Don't** hide progress or remove the persistent Save video alternative after export.
