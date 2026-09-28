//! Whitespace layout after source-bound scalar syntax and semantic admission.
//! Every non-whitespace byte, including comments and punctuation, is retained in order.

mod control_flow;
mod data_ownership;

pub(super) fn format_control_flow(source: &str) -> Option<String> {
    control_flow::format(source)
}

pub(super) fn format_control_flow_bounded(
    source: &str,
    maximum: usize,
) -> Result<String, LayoutError> {
    control_flow::format_bounded(source, maximum)
}

pub(super) use control_flow::LayoutError;
pub(super) fn format_data_ownership_bounded(
    source: &str,
    maximum: usize,
) -> Result<String, LayoutError> {
    data_ownership::format_bounded(source, maximum)
}

pub(super) fn format(source: &str) -> Option<String> {
    let tokens = tokens(source)?;
    let mut output = String::new();
    let mut depth = 0_u8;
    let mut previous = "";
    for token in tokens {
        if token == "}" {
            depth = depth.checked_sub(1)?;
            newline(&mut output);
        }
        if !output.is_empty() && !output.ends_with('\n') && needs_space(previous, token) {
            output.push(' ');
        }
        if output.ends_with('\n') && depth > 0 {
            output.push_str("  ");
        }
        output.push_str(token);
        match token {
            "{" => {
                depth = depth.checked_add(1)?;
                newline(&mut output);
            }
            "}" | ";" => newline(&mut output),
            _ if token.starts_with("//") => newline(&mut output),
            _ => {}
        }
        previous = token;
    }
    if depth != 0 {
        return None;
    }
    newline(&mut output);
    Some(output)
}

fn newline(output: &mut String) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn needs_space(previous: &str, token: &str) -> bool {
    if previous.starts_with("/*") || token.starts_with("/*") || token.starts_with("//") {
        return true;
    }
    if matches!(token, ")" | "," | ":" | ";") || previous == "(" {
        return false;
    }
    if token == "(" {
        return previous == "return" || previous == "+";
    }
    if previous == "-" {
        return false;
    }
    true
}

fn tokens(mut source: &str) -> Option<Vec<&str>> {
    let mut result = Vec::new();
    while !source.is_empty() {
        source = source.trim_start_matches([' ', '\t', '\r', '\n']);
        if source.is_empty() {
            break;
        }
        let end = if source.starts_with("//") {
            source.find(['\r', '\n']).unwrap_or(source.len())
        } else if source.starts_with("/*") {
            source.find("*/")? + 2
        } else if source.starts_with(['(', ')', '{', '}', ',', ':', ';', '+', '-']) {
            1
        } else {
            let end = source
                .find(|c: char| c.is_whitespace() || "(){} ,:;+-/".contains(c))
                .unwrap_or(source.len());
            if end == 0 {
                return None;
            }
            end
        };
        result.push(&source[..end]);
        source = &source[end..];
    }
    Some(result)
}
