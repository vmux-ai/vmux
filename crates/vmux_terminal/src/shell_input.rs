pub fn shell_command_input(command: &str) -> Vec<u8> {
    let mut data = command.as_bytes().to_vec();
    data.push(b'\r');
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_input_appends_carriage_return() {
        assert_eq!(shell_command_input("echo hi"), b"echo hi\r".to_vec());
    }
}
