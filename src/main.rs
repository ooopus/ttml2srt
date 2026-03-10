mod convert;
mod time;
mod vtt;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

/// Convert TTML / VTT subtitle files to SRT format.
///
/// Supports both TTML (.ttml) and WebVTT (.vtt) input.
/// Handles Japanese ruby (furigana) annotations by converting them
/// to a parenthesized fallback form: 漢字(かんじ)
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Input file path(s). If omitted, converts all .ttml/.vtt files in current directory.
    input: Vec<PathBuf>,

    /// Output SRT file path (only valid with a single input file)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// How to render ruby annotations in the SRT output (TTML only)
    #[arg(long, value_enum, default_value_t = RubyMode::Paren)]
    ruby_mode: RubyMode,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum RubyMode {
    /// base(text) — e.g. 屍(しかばね)
    Paren,
    /// Drop ruby text, keep only base
    Drop,
    /// Keep only base characters (strict pairing)
    Keepbase,
}

const SUPPORTED_EXTENSIONS: &[&str] = &["ttml", "vtt"];

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.iter().any(|ext| e.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let ruby_mode = cli.ruby_mode;
    let output_override = cli.output;

    let files: Vec<PathBuf> = if cli.input.is_empty() {
        let mut found: Vec<PathBuf> = std::fs::read_dir(".")
            .context("reading current directory")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| is_supported(p))
            .collect();
        found.sort();
        if found.is_empty() {
            eprintln!("No .ttml/.vtt files found in current directory.");
            return Ok(());
        }
        found
    } else {
        cli.input
    };

    if output_override.is_some() && files.len() > 1 {
        anyhow::bail!("--output can only be used with a single input file");
    }

    let total = files.len();
    let mut ok_count = 0u32;
    let mut err_count = 0u32;

    let mut skip_count = 0u32;

    for (i, input) in files.iter().enumerate() {
        let output = output_override
            .clone()
            .unwrap_or_else(|| input.with_extension("srt"));

        // Skip if output already exists
        if output_override.is_none() && output.exists() {
            eprintln!(
                "[{}/{}] SKIP {} ({}  already exists)",
                i + 1,
                total,
                input.display(),
                output.display()
            );
            skip_count += 1;
            continue;
        }

        match convert_one(input, &output, ruby_mode) {
            Ok(()) => {
                eprintln!(
                    "[{}/{}] {} -> {}",
                    i + 1,
                    total,
                    input.display(),
                    output.display()
                );
                ok_count += 1;
            }
            Err(e) => {
                eprintln!("[{}/{}] ERROR {}: {e:#}", i + 1, total, input.display());
                err_count += 1;
            }
        }
    }

    if total > 1 {
        eprintln!(
            "Done: {ok_count} converted, {skip_count} skipped, {err_count} failed, {total} total."
        );
    }

    if err_count > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn convert_one(input: &Path, output: &Path, ruby_mode: RubyMode) -> Result<()> {
    let content =
        std::fs::read_to_string(input).with_context(|| format!("reading {:?}", input))?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let srt = match ext.as_str() {
        "vtt" => vtt::vtt_to_srt(&content)?,
        _ => convert::ttml_to_srt(&content, ruby_mode)?,
    };

    let mut out = Vec::with_capacity(srt.len() + 3);
    out.extend_from_slice(b"\xEF\xBB\xBF");
    out.extend_from_slice(srt.as_bytes());
    std::fs::write(output, out).with_context(|| format!("writing {:?}", output))?;
    Ok(())
}
