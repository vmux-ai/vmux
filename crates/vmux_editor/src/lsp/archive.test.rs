use super::*;
use std::io::Write;

#[test]
fn kind_detection() {
    assert_eq!(kind_for("x.tar.gz"), ArchiveKind::TarGz);
    assert_eq!(kind_for("x.tgz"), ArchiveKind::TarGz);
    assert_eq!(
        kind_for("rust-analyzer-aarch64-apple-darwin.gz"),
        ArchiveKind::Gz
    );
    assert_eq!(kind_for("x.zip"), ArchiveKind::Zip);
    assert_eq!(kind_for("plain-binary"), ArchiveKind::Raw);
}

#[test]
fn extracts_gz_single_file() {
    let tmp = tempfile::tempdir().unwrap();
    let gz = tmp.path().join("payload.gz");
    {
        let f = std::fs::File::create(&gz).unwrap();
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        enc.write_all(b"binary-contents").unwrap();
        enc.finish().unwrap();
    }
    let dest = tmp.path().join("out");
    extract(&gz, ArchiveKind::Gz, &dest, "server").unwrap();
    assert_eq!(
        std::fs::read(dest.join("server")).unwrap(),
        b"binary-contents"
    );
}

#[test]
fn extracts_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let zp = tmp.path().join("a.zip");
    {
        let f = std::fs::File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        w.start_file("inner.txt", opts).unwrap();
        w.write_all(b"zipped").unwrap();
        w.finish().unwrap();
    }
    let dest = tmp.path().join("out");
    extract(&zp, ArchiveKind::Zip, &dest, "_").unwrap();
    assert_eq!(std::fs::read(dest.join("inner.txt")).unwrap(), b"zipped");
}
