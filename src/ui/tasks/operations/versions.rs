use crate::{config::CoreType, server::install::InstallService};

use super::super::{TaskResult, VersionChoice};
use super::error_text;

pub(in crate::ui::tasks) async fn load(
    core_type: CoreType,
    mc_version: &str,
) -> TaskResult<Vec<VersionChoice>> {
    let service = InstallService::new().map_err(error_text)?;
    let mut choices: Vec<VersionChoice> = match core_type {
        CoreType::Vanilla => service
            .fetch_vanilla_versions()
            .await
            .map_err(error_text)?
            .into_iter()
            .map(|version| VersionChoice {
                label: format!("{} ({})", version.id, version.release_type),
                version: version.id.clone(),
                mc_version: version.id,
            })
            .collect(),
        CoreType::Paper => service
            .fetch_paper_versions()
            .await
            .map_err(error_text)?
            .into_iter()
            .map(|version| VersionChoice {
                label: version.id.clone(),
                version: version.id.clone(),
                mc_version: version.id,
            })
            .collect(),
        CoreType::Forge => service
            .fetch_forge_versions()
            .await
            .map_err(error_text)?
            .into_iter()
            .map(|version| VersionChoice {
                label: version.id.clone(),
                version: version.id,
                mc_version: version.url,
            })
            .collect(),
        CoreType::Neoforge => service
            .fetch_neoforge_versions(mc_version)
            .await
            .map_err(error_text)?
            .into_iter()
            .map(|version| VersionChoice {
                label: version.core_version.clone(),
                version: version.core_version,
                mc_version: version.mc_version,
            })
            .collect(),
        CoreType::Fabric => service
            .fetch_fabric_versions()
            .await
            .map_err(error_text)?
            .into_iter()
            .filter(|version| version.release_type == "release")
            .map(|version| VersionChoice {
                label: version.id.clone(),
                version: version.id.clone(),
                mc_version: version.id,
            })
            .collect(),
        other => return Err(format!("{} 不支持自动安装", other.display_name())),
    };
    choices.truncate(500);
    Ok(choices)
}
