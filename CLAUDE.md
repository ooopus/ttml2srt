# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A Rust CLI tool that converts TTML and WebVTT subtitle files to SRT format. Designed for Japanese subtitles with ruby/furigana annotation support. Output files include a UTF-8 BOM for maximum player compatibility.

## Build & Test Commands

```bash
cargo build                    # dev build
cargo build --release          # release build
cargo test                     # run all tests (unit tests in each module)
cargo test convert::tests      # run only TTML conversion tests
cargo test vtt::tests          # run only VTT conversion tests
cargo test time::tests         # run only time parsing tests
cargo test <test_name>         # run a single test by name
cargo install --path .         # install to ~/.cargo/bin
```

## Architecture

All source lives in `src/` with four modules compiled as a single binary:

- **main.rs** — CLI (clap), batch file discovery, format dispatch by extension (`.ttml` → `convert`, `.vtt` → `vtt`), skip-if-exists logic, UTF-8 BOM output
- **convert.rs** — TTML→SRT conversion using `roxmltree`. Handles two ruby formats:
  - HTML-style `<ruby><rb>…</rb><rt>…</rt></ruby>`
  - TTML-style via `tts:ruby` style attributes (container/base/text roles resolved through a style map)
- **vtt.rs** — VTT→SRT conversion. Line-based parser that strips VTT tags (`<c.japanese>`), positioning metadata, HTML entities (`&lrm;`), and Unicode directional marks
- **time.rs** — Shared time parser (`parse_time`) supporting `HH:MM:SS.mmm` and `123.45s` offset formats, outputs `SrtTime` (Display formats as SRT comma-separated `HH:MM:SS,mmm`)

## Key Design Decisions

- `RubyMode` enum (paren/drop/keepbase) only affects TTML; VTT files already have inline parenthesized ruby
- Batch mode (no args) scans current directory for `.ttml` + `.vtt`, skips files where `.srt` already exists
- Errors on individual files don't abort the batch; exit code 1 if any file failed
- All tests use inline test data (no fixture files)
