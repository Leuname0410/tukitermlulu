//! Value object de ruta remota, tal como la reporta el servidor.

use crate::error::DomainError;

/// Ruta remota POSIX o Windows, sin normalizar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePath(String);

impl RemotePath {
    /// Crea una ruta validando que no esté vacía ni contenga bytes NUL.
    pub fn new(v: &str) -> Result<Self, DomainError> {
        if v.is_empty() {
            return Err(DomainError::EmptyRemotePath);
        }
        if v.contains('\0') {
            return Err(DomainError::InvalidRemotePath);
        }
        Ok(Self(v.to_owned()))
    }

    /// Devuelve la ruta como cadena.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Devuelve el directorio padre, o `None` si la ruta es la raíz.
    pub fn parent(&self) -> Option<RemotePath> {
        let (before, _) = self.0.rsplit_once('/')?;
        if before.is_empty() {
            if self.0 == "/" {
                return None;
            }
            return RemotePath::new("/").ok();
        }
        RemotePath::new(before).ok()
    }

    /// Normaliza una ruta reportada por la terminal a la forma que entiende SFTP en OpenSSH.
    ///
    /// PowerShell y cmd.exe reportan `C:\a\b`, `/c:/a` o `C:`, que se convierten en
    /// `/C:/a/b`, `/C:/a` y `/C:/`. Las rutas POSIX se dejan intactas y no se resuelven `..`.
    pub fn from_terminal(raw: &str) -> Result<Self, DomainError> {
        let bytes = raw.as_bytes();
        let drive = match (bytes.first(), bytes.get(1), bytes.get(2)) {
            (Some(letter), Some(b':'), _) if letter.is_ascii_alphabetic() => Some((*letter, 2)),
            (Some(b'/'), Some(letter), Some(b':')) if letter.is_ascii_alphabetic() => {
                Some((*letter, 3))
            }
            _ => None,
        };
        let Some((letter, tail_at)) = drive else {
            return Self::new(raw);
        };
        let letter = letter.to_ascii_uppercase() as char;
        let tail = raw
            .get(tail_at..)
            .unwrap_or("")
            .trim_start_matches(['/', '\\'])
            .replace('\\', "/");
        Self::new(&format!("/{letter}:/{tail}"))
    }

    /// Añade un nombre a la ruta, rechazando separadores, nombres vacíos, `.` y `..`.
    pub fn join(&self, name: &str) -> Result<RemotePath, DomainError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
        {
            return Err(DomainError::InvalidRemotePath);
        }
        let joined = if self.0 == "/" {
            format!("/{name}")
        } else {
            format!("{}/{name}", self.0)
        };
        RemotePath::new(&joined)
    }
}

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
        assert_eq!(
            RemotePath::new("/a\0b"),
            Err(DomainError::InvalidRemotePath)
        );
    }

    #[test]
    fn should_return_immediate_parent_when_path_has_nested_segments() {
        let path = RemotePath::new("/a/b").expect("ruta válida");
        assert_eq!(
            path.parent().map(|p| p.as_str().to_owned()),
            Some("/a".to_owned())
        );
    }

    #[test]
    fn should_return_root_when_path_is_directly_under_root() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(
            path.parent().map(|p| p.as_str().to_owned()),
            Some("/".to_owned())
        );
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

    #[test]
    fn should_reject_empty_name_when_joining_name() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.join(""), Err(DomainError::InvalidRemotePath));
    }

    #[test]
    fn should_reject_current_directory_when_joining_name() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.join("."), Err(DomainError::InvalidRemotePath));
    }

    #[test]
    fn should_reject_backslash_when_joining_name() {
        let path = RemotePath::new("/a").expect("ruta válida");
        assert_eq!(path.join("..\\..\\x"), Err(DomainError::InvalidRemotePath));
    }

    #[test]
    fn should_normalize_terminal_paths_when_creating_from_terminal() {
        let cases = [
            ("C:\\Users\\x", "/C:/Users/x"),
            ("c:/x", "/C:/x"),
            ("/C:/x", "/C:/x"),
            ("C:", "/C:/"),
            ("/var/log", "/var/log"),
            ("D:\\", "/D:/"),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                RemotePath::from_terminal(raw).map(|path| path.as_str().to_owned()),
                Ok(expected.to_owned()),
                "raw: {raw}"
            );
        }
    }

    #[test]
    fn should_reject_empty_when_creating_from_terminal() {
        assert_eq!(
            RemotePath::from_terminal(""),
            Err(DomainError::EmptyRemotePath)
        );
    }

    #[test]
    fn should_keep_parent_references_when_creating_from_terminal() {
        assert_eq!(
            RemotePath::from_terminal("C:\\a\\..\\b").map(|path| path.as_str().to_owned()),
            Ok("/C:/a/../b".to_owned())
        );
    }
}
