//! DTOs y comandos Tauri del gestor de sesiones.

use serde::{Deserialize, Serialize};
use tauri::State;
use tuki_application::copy_delete_session::{DeleteSession, DuplicateSession};
use tuki_application::edit_session::{CreateSession, UpdateSession};
use tuki_application::session_errors::{PasswordChange, SessionDraft};
use tuki_core::auth::{AuthMethod, KeyRef};
use tuki_core::error::DomainError;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::RepositoryError;
use tuki_core::ports::{SecretKey, SecretStore, SecretString, SessionRepository};
use tuki_core::session::{Folder, FolderId, Session, SessionId};
use zeroize::Zeroizing;

use crate::terminal::{AppState, CommandError, secret_store};

/// Sesión devuelta a la UI; nunca incluye contraseñas ni passphrases.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    id: u64,
    name: String,
    host: String,
    port: u16,
    username: String,
    auth: AuthDto,
    folder: Option<u64>,
    password_saved: bool,
    shell_integration: bool,
}

/// Método de autenticación sin secretos.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthDto {
    Password {
        remember: bool,
    },
    Key {
        path: String,
        #[serde(rename = "hasPassphrase")]
        has_passphrase: bool,
    },
}

/// Datos de una sesión enviados por la UI, validados con los constructores del dominio.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    name: String,
    host: String,
    port: u16,
    username: String,
    auth: AuthDto,
    folder: Option<u64>,
    /// Ausente en clientes antiguos: en ese caso se activa la integración.
    #[serde(default)]
    shell_integration: Option<bool>,
}

/// Cambio de contraseña solicitado al actualizar una sesión.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PasswordInput {
    Keep,
    Set { value: Zeroizing<String> },
    Forget,
}

/// Carpeta devuelta a la UI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    id: u64,
    name: String,
    parent: Option<u64>,
}

fn from_domain(error: DomainError) -> CommandError {
    CommandError::from_session(error.into())
}

fn from_repository(error: RepositoryError) -> CommandError {
    CommandError::from_session(error.into())
}

/// Convierte la entrada validada en una entidad con el identificador indicado.
fn to_session(input: SessionInput, id: u64) -> Result<Session, CommandError> {
    let host = Host {
        hostname: Hostname::new(&input.host).map_err(from_domain)?,
        port: Port::new(input.port).map_err(from_domain)?,
        username: Username::new(&input.username).map_err(from_domain)?,
    };
    let auth = match &input.auth {
        AuthDto::Password { remember } => AuthMethod::Password {
            remember: *remember,
        },
        AuthDto::Key {
            path,
            has_passphrase,
        } => AuthMethod::PublicKey {
            key: KeyRef::new(path).map_err(from_domain)?,
            has_passphrase: *has_passphrase,
        },
    };
    Ok(Session::new(
        SessionId::new(id),
        &input.name,
        host,
        auth,
        input.folder.map(FolderId::new),
    )
    .map_err(from_domain)?
    .with_shell_integration(input.shell_integration.unwrap_or(true)))
}

/// Proyecta la sesión al DTO; `password_saved` ya es un booleano sin el secreto.
fn to_dto(session: &Session, password_saved: bool) -> SessionDto {
    let auth = match &session.auth {
        AuthMethod::Password { remember } => AuthDto::Password {
            remember: *remember,
        },
        AuthMethod::PublicKey {
            key,
            has_passphrase,
        } => AuthDto::Key {
            path: key.path().to_owned(),
            has_passphrase: *has_passphrase,
        },
    };
    SessionDto {
        id: session.id.get(),
        name: session.name.clone(),
        host: session.host.hostname.as_str().to_owned(),
        port: session.host.port.get(),
        username: session.host.username.as_str().to_owned(),
        auth,
        folder: session.folder.map(FolderId::get),
        password_saved,
        shell_integration: session.shell_integration,
    }
}

/// Consulta el almacén solo para saber si hay secreto y lo suelta al instante.
async fn to_dto_with_secrets<S: SecretStore>(
    session: &Session,
    secrets: &S,
) -> Result<SessionDto, CommandError> {
    let password_saved = secrets
        .get(&SecretKey {
            session: session.id,
        })
        .await
        .map_err(|error| CommandError::from_session(error.into()))?
        .is_some();
    Ok(to_dto(session, password_saved))
}

fn to_folder(id: u64, name: &str, parent: Option<u64>) -> Result<Folder, CommandError> {
    Folder::new(FolderId::new(id), name, parent.map(FolderId::new)).map_err(from_domain)
}

fn folder_dto(folder: &Folder) -> FolderDto {
    FolderDto {
        id: folder.id.get(),
        name: folder.name.clone(),
        parent: folder.parent.map(FolderId::get),
    }
}

async fn list<R: SessionRepository, S: SecretStore>(
    repository: R,
    secrets: S,
) -> Result<Vec<SessionDto>, CommandError> {
    let sessions = repository.list_sessions().await.map_err(from_repository)?;
    let mut result = Vec::with_capacity(sessions.len());
    for session in &sessions {
        result.push(to_dto_with_secrets(session, &secrets).await?);
    }
    Ok(result)
}

async fn create<R: SessionRepository, S: SecretStore>(
    repository: R,
    secrets: S,
    input: SessionInput,
    password: Option<Zeroizing<String>>,
) -> Result<SessionDto, CommandError> {
    let id = repository
        .next_session_id()
        .await
        .map_err(from_repository)?;
    let session = to_session(input, id.get())?;
    let draft = SessionDraft {
        name: session.name.clone(),
        host: session.host.clone(),
        auth: session.auth.clone(),
        folder: session.folder,
        shell_integration: session.shell_integration,
    };
    let secret = password
        .as_ref()
        .map(|value| SecretString::new(value.as_str()));
    let saved = CreateSession::new(repository, secrets)
        .execute(draft, secret.as_ref())
        .await
        .map_err(CommandError::from_session)?;
    let password_saved =
        matches!(saved.auth, AuthMethod::Password { remember: true }) && secret.is_some();
    Ok(to_dto(&saved, password_saved))
}

async fn update<R: SessionRepository, S: SecretStore>(
    repository: R,
    secrets: S,
    id: u64,
    input: SessionInput,
    password: PasswordInput,
) -> Result<(), CommandError> {
    let session = to_session(input, id)?;
    let change = match password {
        PasswordInput::Keep => PasswordChange::Keep,
        PasswordInput::Set { value } => PasswordChange::Set(SecretString::new(value.as_str())),
        PasswordInput::Forget => PasswordChange::Forget,
    };
    UpdateSession::new(repository, secrets)
        .execute(&session, change)
        .await
        .map_err(CommandError::from_session)
}

async fn duplicate<R: SessionRepository>(
    repository: R,
    id: u64,
) -> Result<SessionDto, CommandError> {
    let copy = DuplicateSession::new(repository)
        .execute(SessionId::new(id))
        .await
        .map_err(CommandError::from_session)?;
    Ok(to_dto(&copy, false))
}

async fn delete<R: SessionRepository, S: SecretStore>(
    repository: R,
    secrets: S,
    id: u64,
) -> Result<(), CommandError> {
    DeleteSession::new(repository, secrets)
        .execute(SessionId::new(id))
        .await
        .map_err(CommandError::from_session)
}

/// Lista las sesiones con un booleano que indica si su contraseña está guardada.
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionDto>, CommandError> {
    list(state.repository.clone(), secret_store()).await
}

/// Crea una sesión y guarda la contraseña solo si el método lo pide.
#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    input: SessionInput,
    password: Option<Zeroizing<String>>,
) -> Result<SessionDto, CommandError> {
    create(state.repository.clone(), secret_store(), input, password).await
}

/// Actualiza una sesión y aplica el cambio de contraseña solicitado.
#[tauri::command]
pub async fn update_session(
    state: State<'_, AppState>,
    id: u64,
    input: SessionInput,
    password: PasswordInput,
) -> Result<(), CommandError> {
    update(
        state.repository.clone(),
        secret_store(),
        id,
        input,
        password,
    )
    .await
}

/// Duplica una sesión sin copiar su secreto.
#[tauri::command]
pub async fn duplicate_session(
    state: State<'_, AppState>,
    id: u64,
) -> Result<SessionDto, CommandError> {
    duplicate(state.repository.clone(), id).await
}

/// Borra una sesión y su secreto asociado.
#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, id: u64) -> Result<(), CommandError> {
    delete(state.repository.clone(), secret_store(), id).await
}

/// Lista las carpetas del gestor.
#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderDto>, CommandError> {
    let folders = state
        .repository
        .list_folders()
        .await
        .map_err(from_repository)?;
    Ok(folders.iter().map(folder_dto).collect())
}

/// Inserta o actualiza una carpeta; sin id se asigna el siguiente disponible.
#[tauri::command]
pub async fn save_folder(
    state: State<'_, AppState>,
    id: Option<u64>,
    name: String,
    parent: Option<u64>,
) -> Result<FolderDto, CommandError> {
    let id = match id {
        Some(id) => id,
        None => state
            .repository
            .next_folder_id()
            .await
            .map_err(from_repository)?
            .get(),
    };
    let folder = to_folder(id, &name, parent)?;
    state
        .repository
        .save_folder(&folder)
        .await
        .map_err(from_repository)?;
    Ok(folder_dto(&folder))
}

/// Borra una carpeta; el repositorio decide cómo afecta a sus hijas y sesiones.
#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, id: u64) -> Result<(), CommandError> {
    state
        .repository
        .delete_folder(FolderId::new(id))
        .await
        .map_err(from_repository)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tuki_adapters::secret_store::InMemorySecretStore;
    use tuki_adapters::session_repository::SqliteSessionRepository;
    use tuki_application::session_errors::SessionError;
    use tuki_core::auth::{AuthMethod, KeyRef};
    use tuki_core::error::DomainError;
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::{RepositoryError, SecretError};
    use tuki_core::ports::{SecretKey, SecretStore, SecretString};
    use tuki_core::session::{Session, SessionId};
    use zeroize::Zeroizing;

    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[derive(Clone, Default)]
    struct SharedSecrets(Arc<InMemorySecretStore>);

    impl SecretStore for SharedSecrets {
        async fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
            self.0.get(key).await
        }

        async fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
            self.0.set(key, value).await
        }

        async fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.0.delete(key).await
        }
    }

    fn input() -> SessionInput {
        SessionInput {
            name: "prod".to_owned(),
            host: "example.com".to_owned(),
            port: 22,
            username: "alice".to_owned(),
            auth: AuthDto::Password { remember: true },
            folder: None,
            shell_integration: None,
        }
    }

    fn session(id: u64, auth: AuthMethod) -> Session {
        let host = Host {
            hostname: Hostname::new("example.com").unwrap(),
            port: Port::new(22).unwrap(),
            username: Username::new("alice").unwrap(),
        };
        Session::new(SessionId::new(id), "prod", host, auth, None).unwrap()
    }

    #[test]
    fn should_reject_invalid_input_when_converting_to_session() {
        let mut empty_host = input();
        empty_host.host.clear();
        assert_eq!(
            to_session(empty_host, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut zero_port = input();
        zero_port.port = 0;
        assert_eq!(
            to_session(zero_port, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut empty_name = input();
        empty_name.name.clear();
        assert_eq!(
            to_session(empty_name, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut empty_key = input();
        empty_key.auth = AuthDto::Key {
            path: String::new(),
            has_passphrase: false,
        };
        assert_eq!(
            to_session(empty_key, 1).unwrap_err().kind(),
            "invalid_input"
        );
    }

    #[test]
    fn should_hide_stored_password_when_mapping_session_to_dto() {
        let secrets = InMemorySecretStore::default();
        let stored = session(7, AuthMethod::Password { remember: true });
        block_on(secrets.set(
            &SecretKey {
                session: SessionId::new(7),
            },
            &SecretString::new("stored-pw"),
        ))
        .unwrap();

        let dto = block_on(to_dto_with_secrets(&stored, &secrets)).unwrap();
        assert!(dto.password_saved);
        assert!(!serde_json::to_string(&dto).unwrap().contains("stored-pw"));

        let key = session(
            8,
            AuthMethod::PublicKey {
                key: KeyRef::new("/home/alice/.ssh/id_ed25519").unwrap(),
                has_passphrase: true,
            },
        );
        let dto = block_on(to_dto_with_secrets(&key, &secrets)).unwrap();
        assert!(!dto.password_saved);
        assert!(matches!(
            dto.auth,
            AuthDto::Key {
                has_passphrase: true,
                ..
            }
        ));
    }

    #[test]
    fn should_map_every_session_error_when_command_fails() {
        let kind = |error| CommandError::from_session(error).kind().to_owned();
        assert_eq!(
            kind(SessionError::Domain(DomainError::EmptyName)),
            "invalid_input"
        );
        assert_eq!(
            kind(SessionError::Repository(RepositoryError::NotFound)),
            "not_found"
        );
        assert_eq!(
            kind(SessionError::Repository(RepositoryError::Conflict)),
            "conflict"
        );
        assert_eq!(kind(SessionError::Repository(RepositoryError::Io)), "io");
        assert_eq!(kind(SessionError::Secret(SecretError::Unavailable)), "io");
        assert_eq!(kind(SessionError::Secret(SecretError::Io)), "io");
    }

    #[test]
    fn should_reflect_password_changes_when_creating_and_updating() {
        let repository = SqliteSessionRepository::in_memory().unwrap();
        let secrets = SharedSecrets::default();

        let created = block_on(create(
            repository.clone(),
            secrets.clone(),
            input(),
            Some(Zeroizing::new("pw".to_owned())),
        ))
        .unwrap();
        let listed = block_on(list(repository.clone(), secrets.clone())).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].password_saved);

        block_on(update(
            repository.clone(),
            secrets.clone(),
            created.id,
            input(),
            PasswordInput::Forget,
        ))
        .unwrap();
        let listed = block_on(list(repository.clone(), secrets.clone())).unwrap();
        assert!(!listed[0].password_saved);
    }

    #[test]
    fn should_enable_shell_integration_when_input_omits_it() {
        let raw = r#"{"name":"prod","host":"example.com","port":22,"username":"alice","auth":{"kind":"password","remember":true},"folder":null}"#;
        let parsed: SessionInput = serde_json::from_str(raw).unwrap();
        assert!(to_session(parsed, 1).unwrap().shell_integration);
    }

    #[test]
    fn should_expose_shell_integration_in_camel_case_when_mapping_dto() {
        let session =
            session(7, AuthMethod::Password { remember: false }).with_shell_integration(false);
        let dto = to_dto(&session, false);
        assert!(!dto.shell_integration);
        assert_eq!(
            serde_json::to_value(&dto).unwrap()["shellIntegration"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn should_persist_disabled_shell_integration_when_creating_from_input() {
        let repository = SqliteSessionRepository::in_memory().unwrap();
        let secrets = SharedSecrets::default();
        let mut disabled = input();
        disabled.shell_integration = Some(false);

        let created =
            block_on(create(repository.clone(), secrets.clone(), disabled, None)).unwrap();
        assert!(!created.shell_integration);
        let listed = block_on(list(repository, secrets)).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].shell_integration);
    }
}
