//! #7562: Quick Export must write real float EXR, without a web/8-bit intermediate.
#![cfg(not(target_arch = "wasm32"))]

use photocraft_color::SampleType;
use photocraft_engine::Session;
use photocraft_geom::Rect;
use serde_json::json;

fn fixture(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 2, "height": 1, "depth": depth, "background": "transparent"})).unwrap();
    s.edit("fixture", |doc, active| {
        doc.icc_profile = Some(photocraft_cms::Builtin::LinearSrgb.profile().to_bytes());
        let surface = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
        surface.fill_rect(Rect::new(0, 0, 1, 1), &[-0.25, 2.0, 0.1234567, 1.0]);
        surface.fill_rect(Rect::new(1, 0, 2, 1), &[4.0, 0.5, -1.0, 0.5]);
        Ok(())
    })
    .unwrap();
    s
}

#[test]
fn quick_export_exr_preserves_float_samples_and_document() {
    for depth in [8, 16, 32] {
        let mut s = fixture(depth);
        s.execute("prefs.set", json!({"path": "export.quickExportFormat", "value": "exr"})).unwrap();
        // The web preference must not turn a linear float export into a clipped sRGB proxy.
        s.execute("file.export.exportPreferences", json!({"convertToSrgb": true})).unwrap();
        let dir = tempfile::tempdir().unwrap();
        s.active_mut().unwrap().path = Some(dir.path().join("source.pcraft").to_string_lossy().into_owned());
        s.execute("file.export.exportPreferences", json!({"quickExportLocation": "sameFolder"})).unwrap();
        let before = photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap();
        let revision = s.active().unwrap().revision;
        let history = s.active().unwrap().history.past_len();
        let expected = photocraft_compose::flatten(&s.active().unwrap().doc).px;
        if depth == 32 {
            assert_eq!(expected[0], [-0.25, 2.0, 0.1234567, 1.0]);
            assert_eq!(expected[1], [4.0, 0.5, -1.0, 0.5]);
        }
        let result = s.execute("file.export.quickExport", json!({})).unwrap();
        assert_eq!(result["format"], "exr");
        let path = dir.path().join("source.exr");
        assert_eq!(result["path"], path.to_string_lossy().as_ref());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(photocraft_codecs::detect(&bytes), Some(photocraft_codecs::Format::OpenExr));
        if depth == 32 {
            assert_eq!(photocraft_codecs::decode(&bytes).unwrap().sample_type(), photocraft_codecs::SampleType::F32);
        }
        let reopened = photocraft_io::import("source.exr", &bytes).unwrap().document;
        assert_eq!((reopened.size.width, reopened.size.height), (2, 1));
        assert_eq!(reopened.depth, SampleType::F32);
        let actual = photocraft_compose::flatten(&reopened).px;
        for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
            let tolerance = if depth == 32 { 1e-7 } else { 2e-5 };
            assert!((actual - expected).abs() < tolerance, "depth {depth}: {actual} != {expected}");
        }
        // An explicit name chooses the destination, not the encoding.
        let explicit = dir.path().join("explicit.png");
        s.execute("file.export.quickExport", json!({"path": explicit})).unwrap();
        assert_eq!(photocraft_codecs::detect(&std::fs::read(explicit).unwrap()), Some(photocraft_codecs::Format::OpenExr));
        let source = s.active().unwrap();
        assert_eq!(photocraft_format::save_to_bytes(&source.doc, &Default::default()).unwrap(), before);
        assert_eq!(source.revision, revision);
        assert_eq!(source.history.past_len(), history);
    }
}

#[test]
fn quick_export_exr_rejects_missing_document_path_and_bad_destination() {
    let mut s = Session::new();
    s.execute("file.export.exportPreferences", json!({"quickExportFormat": "exr"})).unwrap();
    assert!(s.execute("file.export.quickExport", json!({"path": "unused.exr"})).is_err());
    s.execute("file.new", json!({"width": 1, "height": 1, "depth": 32})).unwrap();
    for params in [json!({}), json!({"path": 42}), json!({"path": ""})] {
        assert!(s.execute("file.export.quickExport", params).is_err());
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(s.execute("file.export.quickExport", json!({"path": dir.path()})).is_err());
}

#[test]
fn existing_quick_export_formats_remain_available() {
    for (format, kind) in [
        ("png", photocraft_codecs::Format::Png),
        ("jpg", photocraft_codecs::Format::Jpeg),
        ("gif", photocraft_codecs::Format::Gif),
        ("webp", photocraft_codecs::Format::WebP),
    ] {
        let mut s = fixture(8);
        s.execute("file.export.exportPreferences", json!({"quickExportFormat": format})).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("export.{format}"));
        s.execute("file.export.quickExport", json!({"path": path})).unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(photocraft_codecs::detect(&bytes), Some(kind));
        assert_eq!(photocraft_codecs::decode(&bytes).unwrap().dimensions(), (2, 1));
    }
}
