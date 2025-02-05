use crate::{errors::Error, killswitch::Channel};
use ethers::prelude::H256;
use nomad_core::TxOutcome;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// KillSwitch response showing success / failure of configuration
/// and tx submission. Gets serialized to json
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Output {
    /// The original command `killswitch` was run with
    pub command: String,
    /// The success / failure message
    pub message: Message,
}

/// A wrapper for success / failure messages
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Message {
    /// An wrapper for a single error we bailed on
    SimpleError(SimpleErrorOutput),
    /// A full results message
    FullMessage(HomesOutput),
}

impl From<Error> for Message {
    /// Convert a blocking `Error` to `Message`
    fn from(error: Error) -> Self {
        Message::SimpleError(SimpleErrorOutput::Result {
            status: Status::Error,
            message: format!("{}", error),
        })
    }
}

/// Simple error output
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SimpleErrorOutput {
    /// Simple errors have a result object
    Result {
        /// Currently, this will always be `Error`
        status: Status,
        /// The error message
        message: String,
    },
}

/// Map of homes by name
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct HomesOutput {
    /// Homes by name
    homes: HashMap<String, HomeOutput>,
}

/// Home
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct HomeOutput {
    /// `Success` if *all* replicas succeeded
    status: Status,
    /// Map of replicas
    message: ReplicasOutput,
}

/// Map of replicas by name
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ReplicasOutput {
    /// Replica by name
    replicas: HashMap<String, ReplicaOutput>,
}

/// Replica
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ReplicaOutput {
    /// Replicas have a result object
    Result {
        /// Replica status
        status: Status,
        /// Will be populated if successful
        tx_hash: Option<H256>,
        /// Will be populated with errors on failure
        message: Option<Vec<String>>,
    },
}

/// Status
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Status {
    /// Successful kill
    Success,
    /// Errors encountered
    Error,
}

/// Build output `Message::FullMessage(Homes)` accepting a set
/// of errored channels as well as successful channels
#[allow(clippy::type_complexity)]
pub(crate) fn build_output_message(
    bad: Vec<(Channel, Vec<Error>)>,
    good: Vec<(Channel, TxOutcome)>,
) -> Message {
    // Failed channels
    let mut replicas = bad
        .into_iter()
        .map(|(channel, errors)| {
            let replica = ReplicaOutput::Result {
                status: Status::Error,
                tx_hash: None,
                message: Some(
                    errors
                        .iter()
                        // Serializing these requires upstream errors to also be
                        // Serialize, just use Display
                        .map(|e| format!("{}", e))
                        .collect::<Vec<String>>(),
                ),
            };
            (channel.clone(), (false, (channel.replica, replica)))
        })
        .collect::<Vec<(_, (_, _))>>();

    // Successful channels
    replicas.extend(good.into_iter().map(|(channel, tx)| {
        let replica = ReplicaOutput::Result {
            status: Status::Success,
            tx_hash: Some(tx.txid),
            message: None,
        };
        (channel.clone(), (true, (channel.replica, replica)))
    }));

    // Map replicas to homes
    let mut homes: HashMap<String, Vec<(bool, (String, ReplicaOutput))>> = HashMap::new();
    for (channel, (success, replica)) in replicas {
        if let Some(replicas) = homes.get_mut(&channel.home) {
            replicas.push((success, replica));
        } else {
            homes.insert(channel.home, vec![(success, replica)]);
        }
    }

    // Full output
    Message::FullMessage(HomesOutput {
        homes: homes
            .into_iter()
            .map(|(home, replicas)| {
                // report error for *any* errors encountered
                let success = replicas.iter().all(|(s, _)| *s);
                (
                    home,
                    HomeOutput {
                        status: if success {
                            Status::Success
                        } else {
                            Status::Error
                        },
                        message: ReplicasOutput {
                            replicas: replicas.into_iter().map(|(_, replica)| replica).collect(),
                        },
                    },
                )
            })
            .collect(),
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use nomad_core::TxOutcome;
    use std::str::FromStr;

    #[test]
    fn it_produces_correct_simple_error_output() {
        let error = Error::BadConfigVar("/bad/path".into());
        let message: Message = error.into();
        let simple_error = match message {
            Message::SimpleError(error) => error,
            _ => panic!("Match error. Should never happen"),
        };
        let json = serde_json::to_string_pretty(&simple_error).unwrap();
        let simple_error: SimpleErrorOutput = serde_json::from_str(&json).unwrap();

        assert_matches!(
            simple_error,
            SimpleErrorOutput::Result { status: Status::Error, message }
                if message == "BadConfigVar: Unable to load config from: /bad/path"
        );
    }

    #[test]
    fn it_produces_correct_bad_output() {
        let channel1 = Channel {
            home: "ethereum".into(),
            replica: "avalanche".into(),
        };
        let channel2 = Channel {
            home: "avalanche".into(),
            replica: "ethereum".into(),
        };
        let error1 = Error::MissingRPC(channel1.home.clone());
        let error2 = Error::MissingAttestationSignerConf(channel1.home.clone());
        let error3 = Error::MissingTxSubmitterConf(channel2.replica.clone());
        let bad = vec![
            (channel1, vec![error1, error2]),
            (channel2.clone(), vec![error3]),
        ];
        let homes = match build_output_message(bad, vec![]) {
            Message::FullMessage(homes) => homes,
            _ => panic!("Match error. Should never happen"),
        };
        let json = serde_json::to_string(&homes).unwrap();

        let result: HomesOutput = serde_json::from_str(&json).unwrap();
        let ethereum = result.homes.get("ethereum").unwrap();
        let avalanche = result.homes.get("avalanche").unwrap();
        let error = format!(
            "{}",
            Error::MissingTxSubmitterConf(channel2.replica.clone())
        );
        assert_matches!(ethereum.status, Status::Error);
        assert_matches!(avalanche.status, Status::Error);
        assert_matches!(
            avalanche.message.replicas.get("ethereum").unwrap(),
            ReplicaOutput::Result {
                message: Some(errors),
                ..
            } if errors.first().unwrap() == &error
        );
    }

    #[test]
    fn it_produces_correct_good_output() {
        let channel1 = Channel {
            home: "ethereum".into(),
            replica: "avalanche".into(),
        };
        let channel2 = Channel {
            home: "avalanche".into(),
            replica: "ethereum".into(),
        };
        let tx1 = TxOutcome {
            txid: H256::from_str(
                "0x1111111111111111111111111111111111111111111111111111111111111111",
            )
            .unwrap(),
        };
        let tx2 = TxOutcome {
            txid: H256::from_str(
                "0x2222222222222222222222222222222222222222222222222222222222222222",
            )
            .unwrap(),
        };
        let good = vec![(channel1, tx1), (channel2, tx2)];
        let homes = match build_output_message(vec![], good) {
            Message::FullMessage(homes) => homes,
            _ => panic!("Match error. Should never happen"),
        };
        let json = serde_json::to_string(&homes).unwrap();

        let result: HomesOutput = serde_json::from_str(&json).unwrap();
        let ethereum = result.homes.get("ethereum").unwrap();
        let avalanche = result.homes.get("avalanche").unwrap();
        assert_matches!(ethereum.status, Status::Success);
        assert_matches!(avalanche.status, Status::Success);
        assert_matches!(
            avalanche.message.replicas.get("ethereum").unwrap(),
            ReplicaOutput::Result { tx_hash: Some(tx), .. } if tx == &tx2.txid
        );
    }

    #[test]
    fn it_produces_correct_mixed_output() {
        let channel1 = Channel {
            home: "ethereum".into(),
            replica: "avalanche".into(),
        };
        let channel2 = Channel {
            home: "avalanche".into(),
            replica: "ethereum".into(),
        };
        let tx = TxOutcome {
            txid: H256::from_str(
                "0x1111111111111111111111111111111111111111111111111111111111111111",
            )
            .unwrap(),
        };
        let error = Error::MissingTxSubmitterConf(channel1.replica.clone());
        let bad = vec![(channel1.clone(), vec![error])];
        let good = vec![(channel2, tx)];
        let homes = match build_output_message(bad, good) {
            Message::FullMessage(homes) => homes,
            _ => panic!("Match error. Should never happen"),
        };
        let json = serde_json::to_string(&homes).unwrap();

        let result: HomesOutput = serde_json::from_str(&json).unwrap();
        let ethereum = result.homes.get("ethereum").unwrap();
        let avalanche = result.homes.get("avalanche").unwrap();
        let error = format!(
            "{}",
            Error::MissingTxSubmitterConf(channel1.replica.clone())
        );
        assert_matches!(ethereum.status, Status::Error);
        assert_matches!(avalanche.status, Status::Success);
        assert_matches!(
            avalanche.message.replicas.get("ethereum").unwrap(),
            ReplicaOutput::Result { tx_hash: Some(t), .. } if t == &tx.txid
        );
        assert_matches!(
            ethereum.message.replicas.get("avalanche").unwrap(),
            ReplicaOutput::Result {
                message: Some(errors),
                ..
            } if errors.first().unwrap() == &error
        );
    }
}
