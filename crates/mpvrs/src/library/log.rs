pub fn append(message: impl AsRef<str>) {
    eprintln!("mpvrs: {}", message.as_ref());
}
