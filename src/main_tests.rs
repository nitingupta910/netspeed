use crate::{CliProgress, OutputFormat, cli_progress_mode};

#[test]
fn text_output_shows_basic_progress_by_default() {
    assert_eq!(
        cli_progress_mode(OutputFormat::Text, false),
        CliProgress::Basic
    );
}

#[test]
fn json_output_stays_quiet_by_default() {
    assert_eq!(
        cli_progress_mode(OutputFormat::Json, false),
        CliProgress::Quiet
    );
}

#[test]
fn requested_progress_is_detailed_for_text_output() {
    assert_eq!(
        cli_progress_mode(OutputFormat::Text, true),
        CliProgress::Detailed
    );
}

#[test]
fn requested_progress_is_detailed_for_json_output() {
    assert_eq!(
        cli_progress_mode(OutputFormat::Json, true),
        CliProgress::Detailed
    );
}
