use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("t2m-bundle-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn bundle(files: &[(&str, &[u8])]) -> ExportBundle {
    ExportBundle {
        files: files
            .iter()
            .map(|(n, b)| (n.to_string(), b.to_vec()))
            .collect(),
        warnings: vec![],
    }
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn t28_1_files_are_written_whole_into_a_folder_that_is_created_and_no_temporary_remains() {
    let dir = scratch("ok").join("nested").join("out");
    let paths = write_bundle(&dir, &bundle(&[("a.mid", b"one"), ("b.scl", b"two")])).unwrap();
    assert_eq!(paths, [dir.join("a.mid"), dir.join("b.scl")]);
    assert_eq!(fs::read(dir.join("a.mid")).unwrap(), b"one");
    assert_eq!(fs::read(dir.join("b.scl")).unwrap(), b"two");
    assert_eq!(listing(&dir), ["a.mid", "b.scl"]);
    let _ = fs::remove_dir_all(dir.parent().unwrap().parent().unwrap());
}

#[test]
fn t28_2_an_existing_file_is_replaced_and_an_empty_bundle_writes_nothing() {
    let dir = scratch("replace");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.mid"), "old").unwrap();
    write_bundle(&dir, &bundle(&[("a.mid", b"new")])).unwrap();
    assert_eq!(fs::read(dir.join("a.mid")).unwrap(), b"new");
    assert_eq!(
        write_bundle(&dir, &bundle(&[])).unwrap(),
        Vec::<PathBuf>::new()
    );
    assert_eq!(listing(&dir), ["a.mid"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t28_3_a_folder_in_the_way_is_refused_before_anything_is_placed() {
    let dir = scratch("blocked");
    fs::create_dir_all(dir.join("b.mid")).unwrap();
    let err = write_bundle(&dir, &bundle(&[("a.mid", b"1"), ("b.mid", b"2")])).unwrap_err();
    assert!(err.to_string().ends_with("b.mid: it is a folder"), "{err}");
    assert_eq!(listing(&dir), ["b.mid"]);
    let _ = fs::remove_dir_all(&dir);
}

// Unix permission bits make a folder unwritable; Windows read-only folders behave differently.
#[cfg(unix)]
#[test]
fn t28_4_an_unwritable_folder_fails_and_stays_as_it_was() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("ro");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("keep.txt"), "x").unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    let err = write_bundle(&dir, &bundle(&[("a.mid", b"1")]));
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(err.unwrap_err().to_string().starts_with("cannot write "));
    assert_eq!(listing(&dir), ["keep.txt"]);
    // A folder that cannot even be created.
    let file = scratch("file");
    fs::write(&file, "x").unwrap();
    let err = write_bundle(&file.join("sub"), &bundle(&[("a.mid", b"1")])).unwrap_err();
    assert!(err.to_string().starts_with("cannot create "), "{err}");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_file(&file);
}
