pub fn cn<I, S>(classes: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut joined = String::new();
    for value in classes {
        let class = value.as_ref().trim();
        if class.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(class);
    }
    joined
}
