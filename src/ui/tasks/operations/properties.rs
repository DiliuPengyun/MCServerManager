use std::sync::Arc;

use crate::{
    AppError, config::ServerId, core::ServerManager, server::properties::ServerProperties,
};

use super::super::TaskResult;
use super::{blocking, error_text};

pub(in crate::ui::tasks) async fn load(
    manager: &Arc<ServerManager>,
    id: ServerId,
) -> TaskResult<(bool, Vec<(String, String)>)> {
    let server = manager.server_config(id).map_err(error_text)?;
    blocking(move || {
        let path = server.working_dir.join("server.properties");
        if !path.exists() {
            return Ok::<_, AppError>((false, Vec::new()));
        }
        let properties = ServerProperties::load(path)?;
        Ok((
            true,
            properties
                .entries()
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        ))
    })
    .await
}

pub(in crate::ui::tasks) async fn save(
    manager: &Arc<ServerManager>,
    id: ServerId,
    entries: Vec<(String, String)>,
) -> TaskResult<Vec<(String, String)>> {
    let server = manager.server_config(id).map_err(error_text)?;
    blocking(move || {
        let path = server.working_dir.join("server.properties");
        let mut properties = if path.exists() {
            ServerProperties::load(&path)?
        } else {
            ServerProperties::default()
        };
        for (key, value) in entries {
            properties.set(&key, &value)?;
        }
        properties.save(&path)?;
        Ok::<_, AppError>(
            properties
                .entries()
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        )
    })
    .await
}
