use super::*;

#[test]
fn ext_of_reads_last_segment_extension() {
    assert_eq!(ext_of("file:///a/b/main.rs"), "rs");
    assert_eq!(ext_of("/x/Photo.PNG"), "png");
    assert_eq!(ext_of("/x/noext"), "");
}

#[test]
fn rust_file_uses_rust_logo() {
    assert_eq!(
        file_icon_kind("file:///x/main.rs", false),
        FileIcon::Logo(lang_logo("rs").unwrap())
    );
}

#[test]
fn dockerfile_by_name_uses_docker_logo() {
    assert_eq!(
        file_icon_kind("/x/Dockerfile", false),
        FileIcon::Logo(lang_logo("dockerfile").unwrap())
    );
}

#[test]
fn directory_uses_folder() {
    assert_eq!(file_icon_kind("/x/src", true), FileIcon::Folder);
}

#[test]
fn image_uses_image() {
    assert_eq!(file_icon_kind("/x/a.png", false), FileIcon::Image);
}

#[test]
fn text_and_code_and_unknown_fall_back() {
    assert_eq!(file_icon_kind("/x/notes.txt", false), FileIcon::Text);
    assert_eq!(file_icon_kind("/x/space.ron", false), FileIcon::Code);
    assert_eq!(file_icon_kind("/x/data.bin", false), FileIcon::File);
}
