use crate::chain::ChainInfo;
use crate::responses::{
    Anchor, AuthCommitteeHotCert, DRep, DRepCredKind, DRepCredential, DRepKind, MirTarget,
    ProposalRedeemer, RegCert, RegDRepCert, ResignCommitteeColdCert, StakeRegDelegCert,
    StakeVoteDelegCert, StakeVoteRegDelegCert, UnRegCert, UnRegDRepCert, UpdateDRepCert, UtxoRef,
    VoteDelegCert, VoteRedeemer, VoteRegDelegCert, Withdrawal,
};
use crate::utils::{assets, datum_option, not_found, reference_script};
use crate::{
    responses::{
        CertRedeemer, Certificates, Datum, ErrorResponse, MintAsset, MintRedeemer, MirCert,
        MirSource, PoolRegCert, PoolRetireCert, Redeemers, Relay, Script, ScriptType,
        SpendRedeemer, StakeDelegCert, StakeRegCert, TimestampedResponse, TransactionInfo, Utxo,
        WdrlRedeemer,
    },
    utils::{self, internal_server_error as ise, internal_server_error_str as ises},
    MapiExtension,
};
use axum::http::HeaderMap;
use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use bech32::ToBase32;
use itertools::Itertools;
use pallas::crypto::hash::Hash;
use pallas::ledger::addresses::{
    Address, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart, StakeAddress, StakePayload,
};
use pallas::ledger::primitives::alonzo::{InstantaneousRewardSource, InstantaneousRewardTarget};
use pallas::ledger::primitives::conway::{
    self, Certificate as PallasCertConway, GovAction, RedeemerTag, Relay as PallasRelay, Voter,
};
use pallas::ledger::primitives::{
    babbage::{Certificate as PallasCertAlonzo, DatumOption, StakeCredential},
    Fragment, ToCanonicalJson,
};
use pallas::ledger::traverse::{
    ComputeHash, Era, MultiEraHeader, MultiEraInput, MultiEraOutput, MultiEraTx, OriginalHash,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use timbre::encoding::decode::decode_block_by_height_value;
use timbre::encoding::{decode_block_by_tx_value, BlockByTxValue};

#[utoipa::path(
    tag = "Transactions",
    get,
    path = "/transactions/{tx_hash}",
    params(
        ("tx_hash" = String, Path, description = "Transaction hash in hex"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Detailed information about the specified transaction",
            body = TimestampedTransactionInfo,
            example = json!({
                "data": {
                    "tx_hash": "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7",
                    "block_hash": "58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be",
                    "block_tx_index": 26,
                    "block_height": 9661308,
                    "block_timestamp": 1702330595i64,
                    "block_absolute_slot": 110764304i64,
                    "block_epoch": 453,
                    "inputs": [{
                        "tx_hash": "498965c4ca9e705e0e4fa90c7b723b6bf5bcdf4362e4843c8b9bd54eaa73c9ad",
                        "index": 0,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 344000000i64
                        }],
                        "address": "addr1zxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uw6j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq6s3z70",
                        "datum": {
                            "type": "hash",
                            "hash": "352956040ebdafc51cb80aed1dcbbbceff03dcfde2eb56cc29511856b5bb476a",
                            "bytes": "d8799fd8799fd8799f581c3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724dffd8799fd8799fd8799f581c09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353ffffffffd8799fd8799f581c3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724dffd8799fd8799fd8799f581c09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353ffffffffd87a80d8799fd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1a4183768bff1a001e84801a001e8480ff",
                            "json": {
                                "constructor": 0,
                                "fields": [{
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "bytes": "3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724d"
                                        }]
                                    }, {
                                        "constructor": 0,
                                        "fields": [{
                                            "constructor": 0,
                                            "fields": [{
                                                "constructor": 0,
                                                "fields": [{
                                                    "bytes": "09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353"
                                                }]
                                            }]
                                        }]
                                    }]
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "bytes": "3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724d"
                                        }]
                                    }, {
                                        "constructor": 0,
                                        "fields": [{
                                            "constructor": 0,
                                            "fields": [{
                                                "constructor": 0,
                                                "fields": [{
                                                    "bytes": "09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353"
                                                }]
                                            }]
                                        }]
                                    }]
                                }, {
                                    "constructor": 1,
                                    "fields": []
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "bytes": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"
                                        }, {
                                            "bytes": "494147"
                                        }]
                                    }, {
                                        "int": 1099134603i64
                                    }]
                                }, {
                                    "int": 2000000
                                }, {
                                    "int": 2000000
                                }]
                            }
                        },
                        "reference_script": null
                    }, {
                        "tx_hash": "c22e28eb033ac63a549e65e0407d374c14bf5805b37f7c8a7b1a0770fe00c656",
                        "index": 0,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1416027292509i64
                        }, {
                            "unit": "0be55d262b29f564998ff81efe21bdc0022621c12f15af08d0f2ddb1bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400",
                            "amount": 1
                        }, {
                            "unit": "13aa2accf2e1561723aa26871e071fdf32c867cff7e7d50ad470d62f4d494e53574150",
                            "amount": 1
                        }, {
                            "unit": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147",
                            "amount": 4615496690137i64
                        }, {
                            "unit": "e4214b7cce62ac6fbba385d164df48e157eae5863521b4b67ca71d86bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400",
                            "amount": 1365147
                        }],
                        "address": "addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz2j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq0xmsha",
                        "datum": {
                            "type": "hash",
                            "hash": "d97ccf3eba5574c513e902ca376bd087c03311c425d481a4ffc38b5c27b8cb4c",
                            "bytes": "d8799fd8799f4040ffd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1b0000018bc1e7de051b0000025339590c7ad8799fd8799fd8799fd8799f581caafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56ffd8799fd8799fd8799f581c52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2ffffffffd87a80ffffff",
                            "json": {
                                "constructor": 0,
                                "fields": [{
                                    "constructor": 0,
                                    "fields": [{
                                        "bytes": ""
                                    }, {
                                        "bytes": ""
                                    }]
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "bytes": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"
                                    }, {
                                        "bytes": "494147"
                                    }]
                                }, {
                                    "int": 1699765280261i64
                                }, {
                                    "int": 2556467678330i64
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "constructor": 0,
                                            "fields": [{
                                                "constructor": 0,
                                                "fields": [{
                                                    "bytes": "aafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56"
                                                }]
                                            }, {
                                                "constructor": 0,
                                                "fields": [{
                                                    "constructor": 0,
                                                    "fields": [{
                                                        "constructor": 0,
                                                        "fields": [{
                                                            "bytes": "52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"
                                                        }]
                                                    }]
                                                }]
                                            }]
                                        }, {
                                            "constructor": 1,
                                            "fields": []
                                        }]
                                    }]
                                }]
                            }
                        },
                        "reference_script": null
                    }, {
                        "tx_hash": "c8fcd44cfa28d6cd9b58ff1cd8c5ce1dc4872ec2655fa723c58ef683610bdc4b",
                        "index": 2,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1455465782i64
                        }, {
                            "unit": "2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030",
                            "amount": 1
                        }],
                        "address": "addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh",
                        "datum": null,
                        "reference_script": null
                    }],
                    "outputs": [{
                        "tx_hash": "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7",
                        "index": 0,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1416367292509i64
                        }, {
                            "unit": "0be55d262b29f564998ff81efe21bdc0022621c12f15af08d0f2ddb1bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400",
                            "amount": 1
                        }, {
                            "unit": "13aa2accf2e1561723aa26871e071fdf32c867cff7e7d50ad470d62f4d494e53574150",
                            "amount": 1
                        }, {
                            "unit": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147",
                            "amount": 4614392059860i64
                        }, {
                            "unit": "e4214b7cce62ac6fbba385d164df48e157eae5863521b4b67ca71d86bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400",
                            "amount": 1365147
                        }],
                        "address": "addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz2j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq0xmsha",
                        "datum": {
                            "type": "hash",
                            "hash": "d97ccf3eba5574c513e902ca376bd087c03311c425d481a4ffc38b5c27b8cb4c",
                            "bytes": "d8799fd8799f4040ffd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1b0000018bc1e7de051b0000025339590c7ad8799fd8799fd8799fd8799f581caafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56ffd8799fd8799fd8799f581c52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2ffffffffd87a80ffffff",
                            "json": {
                                "constructor": 0,
                                "fields": [{
                                    "constructor": 0,
                                    "fields": [{
                                        "bytes": ""
                                    }, {
                                        "bytes": ""
                                    }]
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "bytes": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"
                                    }, {
                                        "bytes": "494147"
                                    }]
                                }, {
                                    "int": 1699765280261i64
                                }, {
                                    "int": 2556467678330i64
                                }, {
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "constructor": 0,
                                            "fields": [{
                                                "constructor": 0,
                                                "fields": [{
                                                    "bytes": "aafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56"
                                                }]
                                            }, {
                                                "constructor": 0,
                                                "fields": [{
                                                    "constructor": 0,
                                                    "fields": [{
                                                        "constructor": 0,
                                                        "fields": [{
                                                            "bytes": "52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"
                                                        }]
                                                    }]
                                                }]
                                            }]
                                        }, {
                                            "constructor": 1,
                                            "fields": []
                                        }]
                                    }]
                                }]
                            }
                        },
                        "reference_script": null
                    }, {
                        "tx_hash": "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7",
                        "index": 1,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 2000000
                        }, {
                            "unit": "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147",
                            "amount": 1104630277
                        }],
                        "address": "addr1qyl8q95sy5s2sn5fz8djdgz945cjyndp5cce986lc9yhyngfmyfg738ffwzf5zwepm4wcjexccxy7ydyp22v5gn8udfste7yl4",
                        "datum": null,
                        "reference_script": null
                    }, {
                        "tx_hash": "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7",
                        "index": 2,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1456673898
                        }, {
                            "unit": "2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030",
                            "amount": 1
                        }],
                        "address": "addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh",
                        "datum": null,
                        "reference_script": null
                    }],
                    "reference_inputs": [],
                    "collateral_inputs": [{
                        "tx_hash": "c8fcd44cfa28d6cd9b58ff1cd8c5ce1dc4872ec2655fa723c58ef683610bdc4b",
                        "index": 2,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1455465782
                        }, {
                            "unit": "2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030",
                            "amount": 1
                        }],
                        "address": "addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh",
                        "datum": null,
                        "reference_script": null
                    }],
                    "collateral_return": {
                        "tx_hash": "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7",
                        "index": 3,
                        "assets": [{
                            "unit": "lovelace",
                            "amount": 1450465782
                        }, {
                            "unit": "2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030",
                            "amount": 1
                        }],
                        "address": "addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh",
                        "datum": null,
                        "reference_script": null
                    },
                    "mint": [],
                    "invalid_before": 110764281,
                    "invalid_hereafter": 110765281,
                    "fee": 791884,
                    "deposit": 0,
                    "certificates": {
                        "stake_registrations": [],
                        "stake_deregistrations": [],
                        "stake_delegations": [],
                        "pool_registrations": [],
                        "pool_retirements": [],
                        "reg_certs": [],
                        "unreg_certs": [],
                        "vote_delegations": [],
                        "stake_vote_delegations": [],
                        "stake_reg_delegations": [],
                        "vote_reg_delegations": [],
                        "stake_vote_reg_delegations": [],
                        "auth_committee_hot_certs": [],
                        "resign_committee_cold_certs": [],
                        "reg_drep_certs": [],
                        "unreg_drep_certs": [],
                        "update_drep_certs": [],
                        "mir_transfers": [],
                    },
                    "withdrawals": [],
                    "additional_signers": ["bcb15ea0ccde1a3e6bca4472724e4017e173f94e97a60d3e682a6e9d"],
                    "scripts_executed": [{
                        "hash": "a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b",
                        "type": "plutusv1",
                        "bytes": "59014c01000032323232323232322223232325333009300e300700213...",
                        "json": null
                    }, {
                        "hash": "e1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec13309",
                        "type": "plutusv1",
                        "bytes": "591e18010000323232323232323232323232323232323232323232323...",
                        "json": null
                    }],
                    "scripts_successful": true,
                    "redeemers": {
                        "spends": [{
                            "script_hash": "a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b",
                            "input": {
                                "tx_hash": "498965c4ca9e705e0e4fa90c7b723b6bf5bcdf4362e4843c8b9bd54eaa73c9ad",
                                "index": 0
                            },
                            "input_index": 0,
                            "data": {
                                "json": {
                                    "constructor": 0,
                                    "fields": []
                                },
                                "bytes": "d87980"
                            },
                            "ex_units": [42061, 14890343]
                        }, {
                            "script_hash": "e1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec13309",
                            "input": {
                                "tx_hash": "c22e28eb033ac63a549e65e0407d374c14bf5805b37f7c8a7b1a0770fe00c656",
                                "index": 0
                            },
                            "input_index": 1,
                            "data": {
                                "json": {
                                    "constructor": 0,
                                    "fields": [{
                                        "constructor": 0,
                                        "fields": [{
                                            "constructor": 0,
                                            "fields": [{
                                                "bytes": "bcb15ea0ccde1a3e6bca4472724e4017e173f94e97a60d3e682a6e9d"
                                            }]
                                        }, {
                                            "constructor": 0,
                                            "fields": [{
                                                "constructor": 0,
                                                "fields": [{
                                                    "constructor": 0,
                                                    "fields": [{
                                                        "bytes": "52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"
                                                    }]
                                                }]
                                            }]
                                        }]
                                    }, {
                                        "int": 2
                                    }]
                                },
                                "bytes": "d8799fd8799fd8799f581cbcb15ea0ccde1a3e6bca4472724e40..."
                            },
                            "ex_units": [2639497, 790336775]
                        }],
                        "mints": [],
                        "withdrawals": [],
                        "certificates": [],
                        "votes": [],
                        "proposals": [],
                    },
                    "metadata": {
                        "674": {
                            "msg": ["Minswap: Order Executed"]
                        }
                    },
                    "size": 9626
                },
                "last_updated": {
                    "timestamp": "2023-12-11 21:36:35",
                    "block_hash": "58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be",
                    "block_slot": 110764304
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TX_INFO", level = "info", skip(config, headers))]
/// Transaction details
///
/// Returns detailed information about a transaction
pub async fn tx_info(
    headers: HeaderMap,
    Path(tx_hash): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let (cbor, mut txn) = polyphony.get_tx_bytes(&tx_hash).await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut txn, &polyphony.tx_by_hash_encoder()?, &chain).await?;

    // ---

    let tx = MultiEraTx::decode(&cbor).map_err(ise)?;

    // ---

    let input_refs = tx
        .inputs()
        .iter()
        .chain(tx.reference_inputs().iter())
        .chain(tx.collateral().iter())
        .map(|x| (**x.hash(), x.index()))
        .collect::<Vec<_>>();

    let input_resolver = utils::fetch_txos(&mut txn, &polyphony, input_refs).await?;

    let resolved_inputs = resolve_inputs(tx.inputs(), &input_resolver);
    let resolved_ref_inputs = resolve_inputs(tx.reference_inputs(), &input_resolver);
    let resolved_coll_inputs = resolve_inputs(tx.collateral(), &input_resolver);

    // ---

    // all scripts found in the transaction wits or inlined in inputs
    let mut scripts_found = Vec::new();

    // scripts which are required to execute for inputs, mints, withdrawals, certs
    let mut scripts_needed = Vec::new();

    // collect datum hashes from resolved inputs and outputs
    let mut datum_hashes = vec![];

    // note which inputs require script execution, store any datum hashes so we can resolve
    for (_, _, txo) in resolved_inputs.iter() {
        let txo = MultiEraOutput::decode(Era::Conway, txo)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, txo))
            .unwrap();

        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }

        if let Ok(Address::Shelley(ref x)) = txo.address() {
            if x.payment().is_script() {
                scripts_needed.push(*x.payment().as_hash());
            }
        }
    }

    // find any datum hashes in the reference inputs and collateral inputs that
    // we need to resolve
    for (_, _, txo) in resolved_ref_inputs
        .iter()
        .chain(resolved_coll_inputs.iter())
    {
        let txo = MultiEraOutput::decode(Era::Conway, txo)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, txo))
            .unwrap();

        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }
    }

    for txo in tx.outputs() {
        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }
    }

    if let Some(txo) = tx.collateral_return() {
        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }
    }

    let mut datum_resolver =
        utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hashes).await?;

    for datum in tx.plutus_data() {
        datum_resolver.insert(*datum.original_hash(), datum.raw_cbor().to_vec());
    }

    // ---

    let inputs = to_mapi_txos(
        resolved_inputs.clone(),
        &headers,
        &datum_resolver,
        &mut scripts_found,
    );

    let reference_inputs = to_mapi_txos(
        resolved_ref_inputs,
        &headers,
        &datum_resolver,
        &mut scripts_found,
    );

    let collateral_inputs =
        to_mapi_txos(resolved_coll_inputs, &headers, &datum_resolver, &mut vec![]);

    // ---

    let tx_hash = *tx.hash();

    let outputs = tx.outputs();

    let labeled_outputs = outputs
        .iter()
        .enumerate()
        .map(|(i, txo)| (tx_hash, i as u64, txo.encode()))
        .collect::<Vec<_>>();

    let outputs = to_mapi_txos(labeled_outputs, &headers, &datum_resolver, &mut vec![]);

    // ---

    let collateral_return = tx.collateral_return().as_ref().map(|txo| {
        to_mapi_txos(
            vec![(tx_hash, tx.outputs().len() as u64, txo.encode())],
            &headers,
            &datum_resolver,
            &mut vec![],
        )[0]
        .clone()
    });

    // ---

    let mut mint = Vec::new();
    let mut policies = Vec::new(); // for redeemer indexing

    for policy in tx.mints() {
        scripts_needed.push(*policy.policy());
        policies.push(*policy.policy());

        for asset in policy.assets() {
            mint.push(MintAsset {
                unit: format!("{}{}", asset.policy(), hex::encode(asset.name())),
                amount: utils::i64_str_conv(asset.mint_coin().unwrap(), &headers),
            });
        }
    }

    // ---

    let invalid_before = tx.validity_start();
    let invalid_hereafter = tx.ttl();

    // ---

    let tx_size = tx.encode().len() as u64;

    let fee = match tx.clone() {
        MultiEraTx::AlonzoCompatible(x, _) => x.transaction_body.fee,
        MultiEraTx::Babbage(x) => x.transaction_body.fee,
        MultiEraTx::Byron(_) => {
            let constant = 155_381_000_000_000;
            let size_coeficient = 43_946_000_000;
            let nanos = constant + (tx_size * size_coeficient);

            let loves = nanos / 1_000_000_000;

            let rem = match nanos % 1_000_000_000 {
                0 => 0u64,
                _ => 1u64,
            };

            loves + rem
        }
        MultiEraTx::Conway(x) => x.transaction_body.fee,
        _ => return Err(ises("missing era")),
    };

    // total deposit change (negative if more refunds than deposits)
    let deposit = if tx.is_valid() {
        let txins: i64 = resolved_inputs
            .clone()
            .iter()
            .map(|(_, _, txo)| {
                let txo = MultiEraOutput::decode(Era::Conway, txo)
                    .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo))
                    .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo))
                    .or_else(|_| MultiEraOutput::decode(Era::Byron, txo))
                    .unwrap();

                txo.value().coin() as i64
            })
            .sum();

        let wdrls = if let Some(w) = tx.withdrawals().as_alonzo() {
            w.values().map(|x| *x as i64).sum()
        } else {
            0
        };

        let txouts: i64 = tx.outputs().iter().map(|x| x.value().coin() as i64).sum();

        (txins + wdrls) - (fee as i64 + txouts)
    } else {
        0
    };

    // --- block

    let block_by_tx_key = polyphony
        .block_by_tx_encoder()?
        .encode_block_by_tx_key(tx_hash);

    let bbt_value_bytes = txn
        .get(block_by_tx_key)
        .await
        .map_err(ise)?
        .ok_or_else(not_found)?;

    let BlockByTxValue { height, index } = decode_block_by_tx_value(&bbt_value_bytes);

    let block_by_height_key = polyphony
        .block_by_height_encoder()?
        .encode_block_by_height_key(height);

    let value_bytes = txn
        .get(block_by_height_key)
        .await
        .map_err(ise)?
        .ok_or_else(not_found)?;

    let info = decode_block_by_height_value(&value_bytes);

    let header = MultiEraHeader::decode(Era::Conway as u8, None, &info.header_bytes)
        .or_else(|_| MultiEraHeader::decode(Era::Babbage as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Alonzo as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Byron as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Byron as u8, Some(0), &info.header_bytes))
        .map_err(ise)?;

    let timestamp = chain.slot_to_unix(header.slot());

    let epoch = match header {
        MultiEraHeader::Byron(ref h) => h.consensus_data.0.epoch,
        MultiEraHeader::EpochBoundary(ref h) => h.consensus_data.epoch_id,
        _ => chain.genesis().absolute_slot_to_relative(header.slot()).0,
    };

    // ---

    let certificates = process_certificates(&tx, &mut scripts_needed, &chain, &headers, epoch);

    // ---

    if let Some(x) = tx.as_conway() {
        if let Some(proposals) = &x.transaction_body.proposal_procedures {
            for proposal in proposals.iter() {
                match &proposal.gov_action {
                    GovAction::ParameterChange(_, _, Some(h)) => scripts_needed.push(*h),
                    GovAction::TreasuryWithdrawals(_, Some(h)) => scripts_needed.push(*h),
                    _ => (),
                }
            }
        }

        if let Some(procedures) = &x.transaction_body.voting_procedures {
            for (voter, _) in procedures.iter() {
                match &voter {
                    Voter::ConstitutionalCommitteeScript(h) => scripts_needed.push(*h),
                    Voter::DRepScript(h) => scripts_needed.push(*h),
                    _ => (),
                }
            }
        }
    }

    // ---

    let mut reward_acnts = Vec::new(); // for redeemer indexing

    let withdrawals = tx.withdrawals();

    let withdrawals = withdrawals
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(racnt, amt)| match Address::from_bytes(racnt).unwrap() {
            Address::Stake(x) => {
                reward_acnts.push(racnt);

                if let StakePayload::Script(h) = x.payload() {
                    scripts_needed.push(*h);
                }

                Withdrawal {
                    stake_address: x.to_bech32().unwrap(),
                    amount: utils::u64_str_conv(amt, &headers),
                }
            }
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();

    // ---

    let additional_signers: Vec<String> = tx
        .required_signers()
        .as_alonzo()
        .unwrap_or(&Default::default())
        .iter()
        .map(|h| h.to_string())
        .collect();

    // ---

    let mut scripts_executed = Vec::new();

    for script in tx.native_scripts() {
        scripts_found.push(Script {
            hash: script.original_hash().to_string(),
            script_type: ScriptType::Native,
            bytes: hex::encode(script.raw_cbor()),
            json: Some(script.to_json()),
        })
    }

    for script in tx.plutus_v1_scripts() {
        scripts_found.push(Script {
            hash: script.compute_hash().to_string(),
            script_type: ScriptType::PlutusV1,
            bytes: hex::encode(script),
            json: None,
        })
    }

    for script in tx.plutus_v2_scripts() {
        scripts_found.push(Script {
            hash: script.compute_hash().to_string(),
            script_type: ScriptType::PlutusV2,
            bytes: hex::encode(script),
            json: None,
        })
    }

    for script in tx.plutus_v3_scripts() {
        scripts_found.push(Script {
            hash: script.compute_hash().to_string(),
            script_type: ScriptType::PlutusV3,
            bytes: hex::encode(script),
            json: None,
        })
    }

    scripts_needed.sort();
    scripts_needed.dedup();

    // fetch the script objects for the scripts which were executed
    for script_hash in scripts_needed {
        let hex_script_hash = hex::encode(script_hash);

        let script = scripts_found.iter().find(|s| s.hash == hex_script_hash);

        match script {
            Some(s) => scripts_executed.push(s.clone()),
            None => {
                return Err(ises(
                    format!("couldn't find needed script: {:?}", script_hash).as_str(),
                ))
            }
        }
    }

    // ---

    let mut spend_rdmrs = Vec::new();
    let mut mint_rdmrs = Vec::new();
    let mut wdrl_rdmrs = Vec::new();
    let mut cert_rdmrs = Vec::new();
    let mut vote_rdmrs = Vec::new();
    let mut propose_rdmrs = Vec::new();

    policies.sort();
    reward_acnts.sort();
    for rdmr in tx.redeemers() {
        match &rdmr.tag() {
            RedeemerTag::Spend => {
                let relevant_input = inputs
                    .get(rdmr.index() as usize)
                    .ok_or_else(|| ises("no input for rdmr index"))?;
                let address = Address::from_bech32(&relevant_input.address).map_err(ise)?;
                let script_hash = match address {
                    Address::Shelley(x) => match x.payment() {
                        ShelleyPaymentPart::Script(h) => hex::encode(h),
                        _ => return Err(ises("spend rdmr address not script")),
                    },
                    _ => return Err(ises("spend rdmr address not shelley")),
                };

                spend_rdmrs.push(SpendRedeemer {
                    script_hash,
                    input: UtxoRef {
                        tx_hash: relevant_input.tx_hash.clone(),
                        index: relevant_input.index,
                    },
                    input_index: rdmr.index() as usize,
                    data: Datum {
                        json: rdmr.data().to_json(),
                        bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                    },
                    ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
                })
            }
            RedeemerTag::Mint => mint_rdmrs.push(MintRedeemer {
                policy: policies[rdmr.index() as usize].to_string(),
                data: Datum {
                    json: rdmr.data().to_json(),
                    bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                },
                ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
            }),
            RedeemerTag::Reward => {
                let racnt_bytes = reward_acnts[rdmr.index() as usize];
                let address = Address::from_bytes(racnt_bytes)
                    .unwrap()
                    .to_bech32()
                    .unwrap();

                wdrl_rdmrs.push(WdrlRedeemer {
                    stake_address: address,
                    data: Datum {
                        json: rdmr.data().to_json(),
                        bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                    },
                    ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
                })
            }
            RedeemerTag::Cert => cert_rdmrs.push(CertRedeemer {
                cert_index: rdmr.index() as usize,
                data: Datum {
                    json: rdmr.data().to_json(),
                    bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                },
                ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
            }),
            RedeemerTag::Vote => vote_rdmrs.push(VoteRedeemer {
                vote_index: rdmr.index() as usize,
                data: Datum {
                    json: rdmr.data().to_json(),
                    bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                },
                ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
            }),
            RedeemerTag::Propose => propose_rdmrs.push(ProposalRedeemer {
                proposal_index: rdmr.index() as usize,
                data: Datum {
                    json: rdmr.data().to_json(),
                    bytes: hex::encode(rdmr.data().encode_fragment().unwrap()),
                },
                ex_units: [rdmr.ex_units().mem, rdmr.ex_units().steps],
            }),
        }
    }

    let redeemers = Redeemers {
        spends: spend_rdmrs,
        mints: mint_rdmrs,
        withdrawals: wdrl_rdmrs,
        certificates: cert_rdmrs,
        votes: vote_rdmrs,
        proposals: propose_rdmrs,
    };

    // ---

    let metadata = if let Some(md) = tx.metadata().as_alonzo() {
        let mut out_json = Map::new();

        for (label, metadatum) in md.iter() {
            let k_string = label.to_string();
            if let Some(v_json) = utils::metadatum_to_json(metadatum.clone()) {
                out_json.insert(k_string, v_json);
            }
        }

        Some(Value::Object(out_json))
    } else {
        None
    };

    // ---

    let tx_info = TransactionInfo {
        // hash, tx index, height, slot, timestamp, epoch
        tx_hash: hex::encode(tx_hash),
        block_hash: hex::encode(header.hash()),
        block_tx_index: index,
        block_height: height,
        block_timestamp: timestamp,
        block_absolute_slot: header.slot(),
        block_epoch: epoch,
        inputs,
        outputs,
        reference_inputs,
        collateral_inputs,
        collateral_return,
        mint,
        invalid_before,
        invalid_hereafter,
        fee,
        deposit,
        certificates,
        withdrawals,
        additional_signers,
        scripts_executed,
        scripts_successful: tx.is_valid(),
        redeemers,
        metadata,
        size: tx_size,
    };

    // ---

    let out = TimestampedResponse {
        data: tx_info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn resolve_inputs<'a>(
    txins: Vec<MultiEraInput<'a>>,
    resolver: &'a HashMap<([u8; 32], u64), Vec<u8>>,
) -> Vec<([u8; 32], u64, Vec<u8>)> {
    txins
        .iter()
        .map(|x| (**x.hash(), x.index()))
        .unique()
        .sorted()
        .map(|(h, i)| (h, i, resolver.get(&(h, i)).unwrap().clone()))
        .collect::<Vec<_>>()
}

fn to_mapi_txos(
    utxos: Vec<([u8; 32], u64, Vec<u8>)>,
    headers: &HeaderMap,
    datum_resolver: &HashMap<[u8; 32], Vec<u8>>,
    found_scripts: &mut Vec<Script>,
) -> Vec<Utxo> {
    let mut out = Vec::new();

    for (tx_hash, index, txo) in utxos {
        let txo = MultiEraOutput::decode(Era::Conway, &txo)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, &txo))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, &txo))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, &txo))
            .unwrap();

        let assets = assets(&txo, headers);
        let datum = datum_option(&txo, Some(datum_resolver));
        let reference_script = reference_script(&txo);

        if let Some(ref script) = reference_script {
            found_scripts.push(script.clone());
        }

        out.push(Utxo {
            tx_hash: hex::encode(tx_hash),
            index,
            assets,
            address: txo.address().unwrap().to_string(),
            datum,
            reference_script,
        })
    }

    out
}

fn cred_to_stake_address(cred: &StakeCredential, chain: &ChainInfo) -> StakeAddress {
    let network = chain.network_id().into();

    let deleg = match cred {
        StakeCredential::AddrKeyhash(h) => ShelleyDelegationPart::Key(*h),
        StakeCredential::ScriptHash(h) => ShelleyDelegationPart::Script(*h),
    };

    let address = ShelleyAddress::new(network, ShelleyPaymentPart::Key([0; 28].into()), deleg);

    address.try_into().unwrap()
}

fn pool_hash_to_b32(pool_kh: [u8; 28]) -> String {
    bech32::encode(
        "pool",
        pool_kh.to_vec().to_base32(),
        bech32::Variant::Bech32,
    )
    .unwrap()
}

fn process_relay(relay: &PallasRelay) -> Relay {
    match relay {
        PallasRelay::SingleHostAddr(p, ipv4, ipv6) => {
            let ipv4 = ipv4.as_ref().map(|x| {
                Ipv4Addr::from(TryInto::<[u8; 4]>::try_into(x.as_slice()).unwrap()).to_string()
            });

            let ipv6 = ipv6.as_ref().map(|x| {
                Ipv6Addr::from(TryInto::<[u8; 16]>::try_into(x.as_slice()).unwrap()).to_string()
            });

            Relay {
                dns: None,
                srv: None,
                ipv4,
                ipv6,
                port: (*p).map(|x| x as i32),
            }
        }
        PallasRelay::SingleHostName(p, dns) => Relay {
            dns: Some(dns.clone()),
            srv: None,
            ipv4: None,
            ipv6: None,
            port: (*p).map(|x| x as i32),
        },
        PallasRelay::MultiHostName(dns) => Relay {
            dns: Some(dns.clone()),
            srv: None,
            ipv4: None,
            ipv6: None,
            port: None,
        },
    }
}

fn process_certificates(
    tx: &MultiEraTx,
    scripts_needed: &mut Vec<Hash<28>>,
    chain: &ChainInfo,
    headers: &HeaderMap,
    epoch: u64,
) -> Certificates {
    let mut stake_registrations = Vec::new();
    let mut stake_deregistrations = Vec::new();
    let mut stake_delegations = Vec::new();
    let mut pool_registrations = Vec::new();
    let mut pool_retirements = Vec::new();
    let mut reg_certs = Vec::new();
    let mut unreg_certs = Vec::new();
    let mut vote_delegations = Vec::new();
    let mut stake_vote_delegations = Vec::new();
    let mut stake_reg_delegations = Vec::new();
    let mut vote_reg_delegations = Vec::new();
    let mut stake_vote_reg_delegations = Vec::new();
    let mut auth_committee_hot_certs = Vec::new();
    let mut resign_committee_cold_certs = Vec::new();
    let mut reg_drep_certs = Vec::new();
    let mut unreg_drep_certs = Vec::new();
    let mut update_drep_certs = Vec::new();
    let mut mir_transfers = Vec::new();

    for (cert_index, cert) in tx.certs().iter().enumerate() {
        if let Some(c) = cert.as_alonzo() {
            match c {
                PallasCertAlonzo::StakeRegistration(cred) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    // TODO: does this require the script witness in Conway? (if so add to scripts_needed)

                    stake_registrations.push(StakeRegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                    });
                }
                PallasCertAlonzo::StakeDelegation(cred, pool_kh) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    stake_delegations.push(StakeDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        pool_id: pool_hash_to_b32(**pool_kh),
                    })
                }
                PallasCertAlonzo::StakeDeregistration(cred) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    stake_deregistrations.push(StakeRegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                    });
                }
                PallasCertAlonzo::PoolRegistration {
                    operator,
                    vrf_keyhash,
                    pledge,
                    cost,
                    margin,
                    reward_account,
                    pool_owners,
                    relays,
                    pool_metadata,
                } => {
                    let reward_address = Address::from_bytes(reward_account)
                        .unwrap()
                        .to_bech32()
                        .unwrap();

                    let owner_addresses = pool_owners
                        .iter()
                        .map(|x| {
                            cred_to_stake_address(&StakeCredential::AddrKeyhash(*x), chain)
                                .to_bech32()
                                .unwrap()
                        })
                        .collect::<Vec<_>>();

                    let margin = (margin.numerator as f64) / (margin.denominator as f64);

                    let relays = relays.iter().map(process_relay).collect();

                    pool_registrations.push(PoolRegCert {
                        cert_index: cert_index as u64,
                        pool_id: pool_hash_to_b32(**operator),
                        from_epoch: epoch + 2,
                        vrf_key_hash: hex::encode(vrf_keyhash),
                        margin: utils::f64_str_conv(margin, headers),
                        fixed_cost: utils::u64_str_conv(*cost, headers),
                        pledge: utils::u64_str_conv(*pledge, headers),
                        reward_address,
                        owner_addresses,
                        relays,
                        metadata_url: pool_metadata.clone().map(|x| x.url),
                        metadata_hash: pool_metadata.clone().map(|x| hex::encode(x.hash.to_vec())),
                    });
                }
                PallasCertAlonzo::PoolRetirement(pool_kh, epoch) => {
                    pool_retirements.push(PoolRetireCert {
                        cert_index: cert_index as u64,
                        pool_id: pool_hash_to_b32(**pool_kh),
                        after_epoch: *epoch as u32,
                    })
                }

                PallasCertAlonzo::GenesisKeyDelegation(_, _, _) => (),
                PallasCertAlonzo::MoveInstantaneousRewardsCert(mir) => {
                    let from = match mir.source {
                        InstantaneousRewardSource::Reserves => MirSource::Reserves,
                        InstantaneousRewardSource::Treasury => MirSource::Treasury,
                    };

                    let (to, other_pot, accounts) = match mir.target {
                        InstantaneousRewardTarget::OtherAccountingPot(x) => {
                            let to = if from == MirSource::Reserves {
                                MirTarget::Treasury
                            } else {
                                MirTarget::Reserves
                            };

                            (to, Some(x), None)
                        }
                        InstantaneousRewardTarget::StakeCredentials(ref creds) => (
                            MirTarget::Accounts,
                            None,
                            Some(
                                creds
                                    .iter()
                                    .map(|(c, a)| Withdrawal {
                                        stake_address: cred_to_stake_address(c, chain)
                                            .to_bech32()
                                            .unwrap(),
                                        amount: utils::i64_str_conv(*a, headers),
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                        ),
                    };

                    mir_transfers.push(MirCert {
                        cert_index: cert_index as u64,
                        from,
                        to,
                        other_pot,
                        accounts,
                    })
                }
            }
        } else if let Some(c) = cert.as_conway() {
            match c {
                PallasCertConway::StakeRegistration(cred) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    stake_registrations.push(StakeRegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                    });
                }
                PallasCertConway::StakeDelegation(cred, pool_kh) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    stake_delegations.push(StakeDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        pool_id: pool_hash_to_b32(**pool_kh),
                    })
                }
                PallasCertConway::StakeDeregistration(cred) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    stake_deregistrations.push(StakeRegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                    });
                }
                PallasCertConway::PoolRegistration {
                    operator,
                    vrf_keyhash,
                    pledge,
                    cost,
                    margin,
                    reward_account,
                    pool_owners,
                    relays,
                    pool_metadata,
                } => {
                    let reward_address = Address::from_bytes(reward_account)
                        .unwrap()
                        .to_bech32()
                        .unwrap();

                    let owner_addresses = pool_owners
                        .iter()
                        .map(|x| {
                            cred_to_stake_address(&StakeCredential::AddrKeyhash(*x), chain)
                                .to_bech32()
                                .unwrap()
                        })
                        .collect::<Vec<_>>();

                    let margin = (margin.numerator as f64) / (margin.denominator as f64);

                    let relays = relays.iter().map(process_relay).collect();

                    pool_registrations.push(PoolRegCert {
                        cert_index: cert_index as u64,
                        pool_id: pool_hash_to_b32(**operator),
                        from_epoch: epoch + 2,
                        vrf_key_hash: hex::encode(vrf_keyhash),
                        margin: utils::f64_str_conv(margin, headers),
                        fixed_cost: utils::u64_str_conv(*cost, headers),
                        pledge: utils::u64_str_conv(*pledge, headers),
                        reward_address,
                        owner_addresses,
                        relays,
                        metadata_url: pool_metadata.clone().map(|x| x.url),
                        metadata_hash: pool_metadata.clone().map(|x| hex::encode(x.hash.to_vec())),
                    });
                }
                PallasCertConway::PoolRetirement(pool_kh, epoch) => {
                    pool_retirements.push(PoolRetireCert {
                        cert_index: cert_index as u64,
                        pool_id: pool_hash_to_b32(**pool_kh),
                        after_epoch: *epoch as u32,
                    })
                }
                PallasCertConway::Reg(cred, deposit) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    reg_certs.push(RegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::UnReg(cred, deposit) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    unreg_certs.push(UnRegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::VoteDeleg(cred, drep) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    let (drep_kind, drep_cred) = match drep {
                        conway::DRep::Key(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Key,
                                bech32: bech32::encode(
                                    "drep",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Script(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Abstain => (DRepKind::Abstain, None),
                        conway::DRep::NoConfidence => (DRepKind::NoConfidence, None),
                    };

                    vote_delegations.push(VoteDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        drep: DRep {
                            kind: drep_kind,
                            credential: drep_cred,
                        },
                    })
                }
                PallasCertConway::StakeVoteDeleg(cred, pool_kh, drep) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    let (drep_kind, drep_cred) = match drep {
                        conway::DRep::Key(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Key,
                                bech32: bech32::encode(
                                    "drep",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Script(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Abstain => (DRepKind::Abstain, None),
                        conway::DRep::NoConfidence => (DRepKind::NoConfidence, None),
                    };

                    stake_vote_delegations.push(StakeVoteDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        pool_id: pool_hash_to_b32(**pool_kh),
                        drep: DRep {
                            kind: drep_kind,
                            credential: drep_cred,
                        },
                    })
                }
                PallasCertConway::StakeRegDeleg(cred, pool_kh, deposit) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    stake_reg_delegations.push(StakeRegDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        pool_id: pool_hash_to_b32(**pool_kh),
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::VoteRegDeleg(cred, drep, deposit) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    let (drep_kind, drep_cred) = match drep {
                        conway::DRep::Key(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Key,
                                bech32: bech32::encode(
                                    "drep",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Script(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Abstain => (DRepKind::Abstain, None),
                        conway::DRep::NoConfidence => (DRepKind::NoConfidence, None),
                    };

                    vote_reg_delegations.push(VoteRegDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        drep: DRep {
                            kind: drep_kind,
                            credential: drep_cred,
                        },
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::StakeVoteRegDeleg(cred, pool_kh, drep, deposit) => {
                    let stake_address = cred_to_stake_address(cred, chain);

                    if let StakeCredential::ScriptHash(h) = cred {
                        scripts_needed.push(*h);
                    }

                    let (drep_kind, drep_cred) = match drep {
                        conway::DRep::Key(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Key,
                                bech32: bech32::encode(
                                    "drep",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Script(h) => (
                            DRepKind::Credential,
                            Some(DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }),
                        ),
                        conway::DRep::Abstain => (DRepKind::Abstain, None),
                        conway::DRep::NoConfidence => (DRepKind::NoConfidence, None),
                    };

                    stake_vote_reg_delegations.push(StakeVoteRegDelegCert {
                        cert_index: cert_index as u64,
                        stake_address: stake_address.to_bech32().unwrap(),
                        pool_id: pool_hash_to_b32(**pool_kh),
                        drep: DRep {
                            kind: drep_kind,
                            credential: drep_cred,
                        },
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::AuthCommitteeHot(cold_cred, hot_cred) => {
                    let cold_cred = match cold_cred {
                        StakeCredential::AddrKeyhash(h) => {
                            bech32::encode("cc_cold", (*h).to_base32(), bech32::Variant::Bech32)
                                .unwrap()
                        }
                        StakeCredential::ScriptHash(h) => {
                            scripts_needed.push(*h);
                            bech32::encode(
                                "cc_cold_script",
                                (*h).to_base32(),
                                bech32::Variant::Bech32,
                            )
                            .unwrap()
                        }
                    };

                    let hot_cred = match hot_cred {
                        StakeCredential::AddrKeyhash(h) => {
                            bech32::encode("cc_hot", (*h).to_base32(), bech32::Variant::Bech32)
                                .unwrap()
                        }
                        StakeCredential::ScriptHash(h) => bech32::encode(
                            "cc_hot_script",
                            (*h).to_base32(),
                            bech32::Variant::Bech32,
                        )
                        .unwrap(),
                    };

                    auth_committee_hot_certs.push(AuthCommitteeHotCert {
                        cert_index: cert_index as u64,
                        committee_cold_credential: cold_cred,
                        committee_hot_credential: hot_cred,
                    })
                }
                PallasCertConway::ResignCommitteeCold(cold_cred, anchor) => {
                    let cold_cred = match cold_cred {
                        StakeCredential::AddrKeyhash(h) => {
                            bech32::encode("cc_cold", (*h).to_base32(), bech32::Variant::Bech32)
                                .unwrap()
                        }
                        StakeCredential::ScriptHash(h) => {
                            scripts_needed.push(*h);
                            bech32::encode(
                                "cc_cold_script",
                                (*h).to_base32(),
                                bech32::Variant::Bech32,
                            )
                            .unwrap()
                        }
                    };

                    let anchor = anchor.as_ref().map(|a| Anchor {
                        url: a.url.clone(),
                        content_hash: a.content_hash.to_string(),
                    });

                    resign_committee_cold_certs.push(ResignCommitteeColdCert {
                        cert_index: cert_index as u64,
                        committee_cold_credential: cold_cred,
                        anchor,
                    })
                }
                PallasCertConway::RegDRepCert(drep_cred, deposit, anchor) => {
                    let drep_cred = match drep_cred {
                        StakeCredential::AddrKeyhash(h) => DRepCredential {
                            kind: DRepCredKind::Key,
                            bech32: bech32::encode(
                                "drep",
                                (*h).to_base32(),
                                bech32::Variant::Bech32,
                            )
                            .unwrap(),
                            hex: h.to_string(),
                        },
                        StakeCredential::ScriptHash(h) => {
                            scripts_needed.push(*h);
                            DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }
                        }
                    };

                    let anchor = anchor.as_ref().map(|a| Anchor {
                        url: a.url.clone(),
                        content_hash: a.content_hash.to_string(),
                    });

                    reg_drep_certs.push(RegDRepCert {
                        cert_index: cert_index as u64,
                        drep_credential: drep_cred,
                        deposit: deposit.to_string(),
                        anchor,
                    })
                }
                PallasCertConway::UnRegDRepCert(drep_cred, deposit) => {
                    let drep_cred = match drep_cred {
                        StakeCredential::AddrKeyhash(h) => DRepCredential {
                            kind: DRepCredKind::Key,
                            bech32: bech32::encode(
                                "drep",
                                (*h).to_base32(),
                                bech32::Variant::Bech32,
                            )
                            .unwrap(),
                            hex: h.to_string(),
                        },
                        StakeCredential::ScriptHash(h) => {
                            scripts_needed.push(*h);
                            DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }
                        }
                    };

                    unreg_drep_certs.push(UnRegDRepCert {
                        cert_index: cert_index as u64,
                        drep_credential: drep_cred,
                        deposit: deposit.to_string(),
                    })
                }
                PallasCertConway::UpdateDRepCert(drep_cred, anchor) => {
                    let drep_cred = match drep_cred {
                        StakeCredential::AddrKeyhash(h) => DRepCredential {
                            kind: DRepCredKind::Key,
                            bech32: bech32::encode(
                                "drep",
                                (*h).to_base32(),
                                bech32::Variant::Bech32,
                            )
                            .unwrap(),
                            hex: h.to_string(),
                        },
                        StakeCredential::ScriptHash(h) => {
                            scripts_needed.push(*h);
                            DRepCredential {
                                kind: DRepCredKind::Script,
                                bech32: bech32::encode(
                                    "drep_script",
                                    (*h).to_base32(),
                                    bech32::Variant::Bech32,
                                )
                                .unwrap(),
                                hex: h.to_string(),
                            }
                        }
                    };

                    let anchor = anchor.as_ref().map(|a| Anchor {
                        url: a.url.clone(),
                        content_hash: a.content_hash.to_string(),
                    });

                    update_drep_certs.push(UpdateDRepCert {
                        cert_index: cert_index as u64,
                        drep_credential: drep_cred,
                        anchor,
                    })
                }
            }
        }
    }

    Certificates {
        stake_registrations,
        stake_deregistrations,
        stake_delegations,
        pool_registrations,
        pool_retirements,
        reg_certs,
        unreg_certs,
        vote_delegations,
        stake_vote_delegations,
        stake_reg_delegations,
        vote_reg_delegations,
        stake_vote_reg_delegations,
        auth_committee_hot_certs,
        resign_committee_cold_certs,
        reg_drep_certs,
        unreg_drep_certs,
        update_drep_certs,

        mir_transfers,
    }
}
