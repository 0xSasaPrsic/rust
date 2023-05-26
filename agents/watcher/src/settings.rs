//! Configuration

use nomad_base::decl_settings;
use nomad_xyz_configuration::agent::watcher::WatcherConfig;

decl_settings!(Watcher, WatcherConfig,);

#[cfg(test)]
mod test {
    use super::*;
    use nomad_base::{get_remotes_from_env, NomadAgent};
    use nomad_test::test_utils;
    use nomad_xyz_configuration::{core::CoreDeploymentInfo, AgentSecrets};

    #[ignore]
    #[tokio::test]
    #[serial_test::serial]
    async fn it_builds_settings_from_env() {
        test_utils::run_test_with_env("../../fixtures/env.test", || async move {
            let run_env = dotenv::var("RUN_ENV").unwrap();
            let agent_home = dotenv::var("AGENT_HOME_NAME").unwrap();

            let settings = WatcherSettings::new().await.unwrap();

            let config = nomad_xyz_configuration::get_builtin(&run_env).unwrap();

            let remotes = get_remotes_from_env!(agent_home, config);
            let mut networks = remotes.clone();
            networks.insert(agent_home.clone());

            let secrets = AgentSecrets::from_env(&networks).unwrap();

            settings
                .base
                .validate_against_config_and_secrets(
                    crate::Watcher::AGENT_NAME,
                    &agent_home,
                    &remotes,
                    config,
                    &secrets,
                )
                .unwrap();

            let agent_config = &config.agent().get("ethereum").unwrap().watcher;
            assert_eq!(settings.agent.interval, agent_config.interval);
            assert_eq!(settings.base.attestation_signer, secrets.attestation_signer);

            let home_connections = &config
                .protocol()
                .networks
                .get(&agent_home)
                .expect("!networks")
                .connections;

            let manager_setups = settings.as_ref().managers.as_ref().unwrap();

            for remote_network in home_connections {
                let manager_setup = manager_setups.get(remote_network).unwrap();

                let config_manager_domain = config
                    .protocol()
                    .get_network(remote_network.to_owned().into())
                    .unwrap();

                assert_eq!(manager_setup.name, config_manager_domain.name);
                assert_eq!(manager_setup.domain, config_manager_domain.domain);
                assert_eq!(
                    manager_setup.page_settings.page_size,
                    config_manager_domain.specs.index_page_size
                );
                assert_eq!(
                    manager_setup.finality,
                    config_manager_domain.specs.finalization_blocks
                );

                let config_manager_core = config.core().get(remote_network).unwrap();
                match config_manager_core {
                    CoreDeploymentInfo::Ethereum(core) => {
                        assert_eq!(
                            manager_setup.address.unwrap(),
                            core.x_app_connection_manager,
                        );
                        assert_eq!(manager_setup.page_settings.from, core.deploy_height);
                    }
                    CoreDeploymentInfo::Substrate(_) => {
                        unimplemented!("Substrate connection manager not yet implemented")
                    }
                }

                let manager_chain_conf = secrets.rpcs.get(remote_network).unwrap();
                assert_eq!(&manager_setup.chain, manager_chain_conf);
            }
        })
        .await
    }
}
