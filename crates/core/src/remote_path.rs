//! Value object de ruta remota, tal como la reporta el servidor.

#[cfg(test)]
mod tests {
    use super::RemotePath;
    use crate::error::DomainError;

    #[test]
    fn should_reject_empty_when_creating_remote_path() {
        assert_eq!(RemotePath::new(""), Err(DomainError::EmptyRemotePath));
    }

    #[test]
    fn should_reject_nul_byte_when_creating_remote_path() {
        assert_eq!(RemotePath::new("/a\0b"), Err(DomainError::InvalidRemotePath));
    }

    #[test]
    fn should_return_immediate_parent_when_path_has_nested_segments() {
        let path = RemotePath::new("/a/b").expect("ruta válida");
        assert_eq!(path.parent().map(|p| p.as_str().to_owned()), Some("/a".to_owned()));
    }

    #[test]
    fn should_return_root_when_path_is_directly_under_root() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.parent().map(|p| p.as_str().to_owned()), Some("/".to_owned()));
    }

    #[test]
    fn should_return_none_when_path_is_root() {
        let path = RemotePath::new("/").expect("ruta válida");
        assert_eq!(path.parent(), None);
    }

    #[test]
    fn should_append_name_when_joining_to_path() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(
            path.join("b").map(|p| p.as_str().to_owned()),
            Ok("/a/b".to_owned())
        );
    }

    #[test]
    fn should_avoid_double_slash_when_joining_to_root() {
        let path = RemotePath::new("/").expect("ruta válida");
        assert_eq!(
            path.join("b").map(|p| p.as_str().to_owned()),
            Ok("/b".to_owned())
        );
    }

    #[test]
    fn should_reject_slash_when_joining_name() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.join("b/c"), Err(DomainError::InvalidRemotePath));
    }

    #[test]
    fn should_reject_parent_reference_when_joining_name() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.join(".."), Err(DomainError::InvalidRemotePath));
    }
}
