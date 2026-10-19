# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Product Purpose

Quick Recorded eXchange encodes a file as FQ1 QR frames/video and recovers a file
from recorded video or camera QR frames, verifying SHA-256 before download.

## Operating Context

The user runs this browser app on a local network. The server only serves static
assets; selected files and media are processed entirely in the browser using Rust
WASM and native browser APIs. Long-running work must not have artificial deadlines.

## Capabilities and Constraints

Preserve the existing FQ1 wire format, file/video roundtrip, offline asset caching,
cancellation, and keep-screen-on controls. Browser/OS suspension and termination
cannot be overridden by a website. Keep the application extremely efficient,
without a new UI framework or external runtime resources.

## Brand Commitments

The user explicitly requests Swiss and minimalist design, black and white with
reversible light/dark modes, and direct, calm interaction. Show either Encode or
Receive at one time, reducing cognitive load. Keep the QR artifact black on white
in both themes. Earlier decorative green card layouts are rejected references.

## Product Principles

- Client-side processing and no payload uploads.
- Show the task in progress and clear ways to complete, cancel, or recover it.
- Display only one workspace at a time and retain in-tab work across mode changes.
- Prefer accessible native controls, neutral typography, and low runtime overhead.

## Evidence on Hand

The repository's Rust protocol tests and real-browser roundtrip, offline, camera
pipeline, and wake-lock tests. Physical device-specific behavior needs validation
on the intended hardware; do not invent compatibility guarantees.
