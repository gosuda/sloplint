//! PowerShell 7.7.0 permits comments and Unicode whitespace before a
//! continued pipe, but rejects a blank line and non-whitespace format marks.

use super::powershell::{run, run_clean};

#[test]
fn comments_and_whitespace_keep_a_grouped_pipeline_connected() {
    for source in [
        "if (Get-Date\n# Filter below\n| Select-Object -First 1) {}\n",
        "(Get-Date\n# Filter below\n| Write-Output)\n",
        "(Get-Date\n# first\n# second\n| Write-Output)\n",
        "(Get-Date # first\n| Write-Output)\n",
        "(Get-Date\n<# filter #>\n| Write-Output)\n",
        "(Get-Date <# example <# #> | Write-Output)\n",
        "Get-Date\n<# note\n more #>\n| Select-Object -First 1\n",
        "(Get-Date\n<# before\n\nafter #>\n| Write-Output)\n",
        "foreach ($x in Get-Date\n# first\n| Write-Output) {}\n",
        "while (Get-Date\n# first\n| Write-Output) {}\n",
        "switch (Get-Date\n# first\n| Write-Output) { default {} }\n",
        "(Get-Date\n\u{a0}| Write-Output)\n",
        "(Get-Date\n\u{85}| Write-Output)\n",
        "(Get-Date\n\u{2003}| Write-Output)\n",
        "(Get-Date\n\u{202f}| Write-Output)\n",
        "(Get-Date\n\u{000b}| Write-Output)\n",
        "(Get-Date\n\u{c}| Write-Output)\n",
    ] {
        for input in [source.to_owned(), source.replace('\n', "\r\n")] {
            run_clean(&input);
        }
    }
}

#[test]
fn blank_lines_and_format_marks_do_not_continue_a_grouped_pipeline() {
    for source in [
        "(Get-Date\n<# before <# nested #> after #>\n| Write-Output)\n",
        "(Get-Date\n\n| Write-Output)\n",
        "(Get-Date\n\n<# note #> | Write-Output)\n",
        "(Get-Date\n# first\n\n| Write-Output)\n",
        "(Get-Date\n\n# first\n| Write-Output)\n",
        "(Get-Date\n   \n| Write-Output)\n",
        "(Get-Date\n\u{feff}| Write-Output)\n",
        "(Get-Date\n\u{200b}| Write-Output)\n",
        "(Get-Date\n\u{2060}| Write-Output)\n",
    ] {
        for input in [source.to_owned(), source.replace('\n', "\r\n")] {
            assert!(run(&input).parse_error.is_some(), "{input}");
        }
    }
}
