#![feature(fn_traits)]
#![feature(never_type)]
#![feature(specialization)]
#![feature(trait_alias)]
#![feature(trivial_bounds)]
#![allow(incomplete_features)]

use bitcoin::hashes::hex::FromHex;
use bitcoin::hashes::sha256d::Hash;
use bitcoin::{BlockHash, BlockHeader, TxMerkleNode};
use chrono::{TimeZone, Utc};
use nomic::bitcoin::adapter::Adapter;
use nomic::bitcoin::header_queue::{Config, HeaderQueue, WrappedHeader};
use orga::context::Context;
use orga::plugins::Paid;

fn trusted_config() -> Config {
    Config {
        max_length: 2000,
        max_time_increase: 8 * 60 * 60,
        trusted_height: 42,
        retarget_interval: 2016,
        target_spacing: 10 * 60,
        target_timespan: 2016 * (10 * 60),
        max_target: 0x1d00ffff,
        retargeting: true,
        min_difficulty_blocks: false,
        encoded_trusted_header: vec![
            1, 0, 0, 0, 139, 82, 187, 215, 44, 47, 73, 86, 144, 89, 245, 89, 193, 177, 121, 77,
            229, 25, 46, 79, 125, 109, 43, 3, 199, 72, 43, 173, 0, 0, 0, 0, 131, 228, 248, 169,
            213, 2, 237, 12, 65, 144, 117, 193, 171, 181, 213, 111, 135, 138, 46, 144, 121, 229,
            97, 43, 251, 118, 162, 220, 55, 217, 196, 39, 65, 221, 104, 73, 255, 255, 0, 29, 43,
            144, 157, 214,
        ]
        .try_into()
        .unwrap(),
    }
}

fn valid_header_43() -> WrappedHeader {
    let stamp = Utc.with_ymd_and_hms(2009, 1, 10, 17, 44, 37).unwrap();
    let header = BlockHeader {
        version: 0x1,
        prev_blockhash: BlockHash::from_hash(
            Hash::from_hex("00000000314e90489514c787d615cea50003af2023796ccdd085b6bcc1fa28f5")
                .unwrap(),
        ),
        merkle_root: TxMerkleNode::from_hash(
            Hash::from_hex("2f5c03ce19e9a855ac93087a1b68fe6592bcf4bd7cbb9c1ef264d886a785894e")
                .unwrap(),
        ),
        time: stamp.timestamp() as u32,
        bits: 486_604_799,
        nonce: 2_093_702_200,
    };
    WrappedHeader::new(Adapter::new(header), 43)
}

fn invalid_replacement_header_43() -> WrappedHeader {
    let stamp = Utc.with_ymd_and_hms(2009, 1, 10, 17, 50, 0).unwrap();
    let header = BlockHeader {
        version: 0x1,
        prev_blockhash: BlockHash::from_hash(
            Hash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
        ),
        merkle_root: TxMerkleNode::from_hash(
            Hash::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .unwrap(),
        ),
        time: stamp.timestamp() as u32,
        bits: 486_604_799,
        nonce: 1,
    };
    WrappedHeader::new(Adapter::new(header), 43)
}

#[test]
fn failed_same_height_replacement_truncates_queue_before_returning_error() {
    Context::add(Paid::default());

    let mut queue = HeaderQueue::default();
    queue.configure(trusted_config()).unwrap();
    let trusted_work = *queue.get_by_height(42).unwrap().unwrap().chain_work;
    let header_43_work = valid_header_43().work();
    queue.add_into_iter([valid_header_43()]).unwrap();
    assert_eq!(queue.height().unwrap(), 43);

    let err = queue
        .add_into_iter([invalid_replacement_header_43()])
        .expect_err("invalid replacement should fail");
    let err_text = err.to_string();
    assert!(
        err_text.contains("incorrect previous block hash"),
        "unexpected error: {err_text}"
    );

    assert_eq!(
        queue.height().unwrap(),
        42,
        "HeaderQueue was truncated by pop_back_to before validation returned Err",
    );

    queue.add_into_iter([valid_header_43()]).unwrap();
    let readded = queue.get_by_height(43).unwrap().unwrap();
    let expected_chain_work = trusted_work + header_43_work;
    let inflated_chain_work = expected_chain_work + header_43_work;
    assert_eq!(
        *readded.chain_work, inflated_chain_work,
        "re-added header was scored from stale current_work left by the failed replacement"
    );
    assert!(
        *readded.chain_work > expected_chain_work,
        "re-added header should not have more chain work than trusted + one header"
    );
}
