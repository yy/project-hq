use std::collections::BTreeMap;

fn strip_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

fn parse_value(raw: &str) -> String {
    let value = raw.trim();

    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

/// Split a markdown document into raw frontmatter text and body slices.
pub fn split_frontmatter(text: &str) -> Result<(&str, &str), &'static str> {
    let text = strip_utf8_bom(text);

    if !text.starts_with("---") {
        return Err("No frontmatter");
    }

    let rest = &text[3..];
    if !(rest.starts_with('\n') || rest.starts_with("\r\n")) {
        return Err("Malformed frontmatter");
    }

    let mut offset = 0;
    while offset < rest.len() {
        let line_end = rest[offset..]
            .find('\n')
            .map(|i| offset + i)
            .unwrap_or(rest.len());
        let line = rest[offset..line_end].trim_end_matches('\r');

        if line == "---" {
            return Ok((&rest[..offset], &rest[line_end..]));
        }

        if line_end == rest.len() {
            break;
        }
        offset = line_end + 1;
    }

    Err("Malformed frontmatter")
}

/// Parse simple `key: value` fields from frontmatter.
pub fn parse_frontmatter(text: &str) -> Option<BTreeMap<String, String>> {
    let (fm_text, _) = split_frontmatter(text).ok()?;

    let mut fields = BTreeMap::new();
    for line in fm_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = parse_value(value);
            if !value.is_empty() {
                fields.insert(key, value);
            }
        }
    }

    if fields.contains_key("title") && fields.contains_key("status") {
        Some(fields)
    } else {
        None
    }
}
