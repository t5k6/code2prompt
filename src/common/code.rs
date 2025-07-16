/// Wraps code in a markdown block, optionally with language extension and line numbers.
pub fn wrap(code: &str, ext: &str, line_numbers: bool, no_block: bool) -> String {
    if no_block {
        return code.to_owned();
    }
    let mut body = String::new();
    if line_numbers {
        for (i, line) in code.lines().enumerate() {
            body.push_str(&format!("{:4} | {}\n", i + 1, line));
        }
    } else {
        body.push_str(code);
    }
    format!("```{ext}\n{body}```")
}
