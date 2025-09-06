use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::{cell::RefCell, collections::HashMap};

#[cfg(target_os = "macos")]
fn normalize_tag_label(s: &str) -> String {
    s.split_once('\n').map(|(h, _)| h).unwrap_or(s).to_lowercase()
}

#[cfg(target_os = "macos")]
fn parse_mdls_tags_output(s: &str) -> Vec<String> {
    let t = s.trim();
    if t == "(null)" || t.is_empty() { return vec![]; }
    let inner = t.strip_prefix('(').and_then(|u| u.strip_suffix(')')).unwrap_or(t);
    let mut tags = Vec::new();
    for line in inner.lines() {
        for part in line.split(',') {
            let token = part.trim().trim_matches('"').trim();
            if token.is_empty() || token == "(null)" { continue; }
            tags.push(normalize_tag_label(token));
        }
    }
    tags
}

#[cfg(target_os = "macos")]
fn mdls_tags(path: &Path) -> Option<Vec<String>> {
    use std::process::Command;
    let s = path.to_str()?;
    let output = Command::new("mdls")
        .args(["-raw", "-name", "kMDItemUserTags", s])
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(parse_mdls_tags_output(text.trim()))
}

#[cfg(target_os = "macos")]
fn parse_xattr_tags(data: &[u8]) -> Option<Vec<String>> {
    use std::io::Cursor;
    let value = plist::Value::from_reader(Cursor::new(data)).ok()?;
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        if let Some(s) = v.as_string() { out.push(normalize_tag_label(s)); }
    }
    Some(out)
}

#[cfg(target_os = "macos")]
fn xattr_tags(path: &Path) -> Option<Vec<String>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    const ATTR: &str = "com.apple.metadata:_kMDItemUserTags";
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let c_name = CString::new(ATTR).ok()?;
    let size = unsafe { libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
    if size <= 0 { return Some(Vec::new()); }
    let mut buf = vec![0u8; size as usize];
    let read = unsafe { libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), buf.as_mut_ptr() as *mut _, buf.len(), 0, 0) };
    if read <= 0 { return Some(Vec::new()); }
    buf.truncate(read as usize);
    parse_xattr_tags(&buf).or_else(|| Some(Vec::new()))
}

#[cfg(target_os = "macos")]
thread_local! {
    static TAGS_CACHE: RefCell<HashMap<PathBuf, Vec<String>>> = RefCell::new(HashMap::new());
}

#[cfg(target_os = "macos")]
fn tags_for_cached(path: &Path) -> Option<Vec<String>> {
    TAGS_CACHE.with(|c| {
        if let Some(v) = c.borrow().get(path) { return Some(v.clone()); }
        let tags = xattr_tags(path).or_else(|| mdls_tags(path));
        if let Some(t) = tags {
            c.borrow_mut().insert(path.to_path_buf(), t.clone());
            Some(t)
        } else {
            None
        }
    })
}

#[cfg(target_os = "macos")]
pub fn tags_for(path: &Path) -> Vec<String> { tags_for_cached(path).unwrap_or_default() }

#[cfg(not(target_os = "macos"))]
pub fn tags_for(_path: &Path) -> Vec<String> { Vec::new() }

