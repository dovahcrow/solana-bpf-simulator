mod errors;
mod executor;
mod loader;

pub use errors::SolanaSimulatorError;
pub use executor::{ExecutionRecord, SBPFMessageExecutor};
pub use loader::AccountLoader;

use std::cmp::Ordering;

use solana_program_runtime::loaded_programs::{self, BlockRelation};
use solana_sdk::{
    epoch_schedule::DEFAULT_SLOTS_PER_EPOCH, pubkey, pubkey::Pubkey, slot_history::Slot,
    stake_history::Epoch,
};

pub const FEATURES: &'static [Pubkey] = &[
    pubkey!("E3PHP7w8kB7np3CTQ1qQ2tW3KCtjRSXBQgW9vM2mWv2Y"),
    pubkey!("E5JiFDQCwyC6QfT9REFyMpfK2mHcmv1GUDySU1Ue7TYv"),
    pubkey!("4kpdyrcj5jS47CZb2oJGfVxjYbsMm2Kx97gFyZrxxwXz"),
    pubkey!("GaBtBJvmS4Arjj5W1NmFcyvPjsHN38UGYDq2MDwbs9Qu"),
    pubkey!("4RWNif6C2WCNiKVW7otP4G7dkmkHGyKQWRpuZ1pxKU5m"),
    pubkey!("GE7fRxmW46K6EmCD9AMZSbnaJ2e3LfqCZzdHi9hmYAgi"),
    pubkey!("7XRJcS5Ud5vxGB54JbK9N2vBZVwnwdBNeJW1ibRgD9gx"),
    pubkey!("BzBBveUDymEYoYzcMWNQCx3cd4jQs7puaVFHLtsbB6fm"),
    pubkey!("BL99GYhdjjcv6ys22C9wPgn2aTVERDbPHHo4NbS3hgp7"),
    pubkey!("GvDsGDkH5gyzwpDhxNixx8vtx1kwYHH13RiNAPw27zXb"),
    pubkey!("3ccR6QpxGYsAbWyfevEtBNGfWV4xBffxRj2tD6A9i39F"),
    pubkey!("D4jsDcXaqdW8tDAWn8H4R25Cdns2YwLneujSL1zvjW6R"),
    pubkey!("BcWknVcgvonN8sL4HE4XFuEVgfcee5MwxWPAgP6ZV89X"),
    pubkey!("BrTR9hzw4WBGFP65AJMbpAo64DcA3U6jdPSga9fMV5cS"),
    pubkey!("FToKNBYyiF4ky9s8WsmLBXHCht17Ek7RXaLZGHzzQhJ1"),
    pubkey!("3E3jV7v9VcdJL8iYZUMax9DiDno8j7EWUVbhm9RtShj2"),
    pubkey!("C5fh68nJ7uyKAuYZg2x9sEQ5YrVf3dkW6oojNBSc3Jvo"),
    pubkey!("EBeznQDjcPG8491sFsKZYBi5S5jTVXMpAKNDJMQPS2kq"),
    pubkey!("EVW9B5xD9FFK7vw1SBARwMA4s5eRo5eKJdKpsBikzKBz"),
    pubkey!("SAdVFw3RZvzbo6DvySbSdBnHN4gkzSTH9dSxesyKKPj"),
    pubkey!("meRgp4ArRPhD3KtCY9c5yAf2med7mBLsjKTPeVUHqBL"),
    pubkey!("6RvdSWHh8oh72Dp7wMTS2DBkf3fRPtChfNrAo3cZZoXJ"),
    pubkey!("BKCPBQQBZqggVnFso5nQ8rQ4RwwogYwjuUt9biBjxwNF"),
    pubkey!("265hPS8k8xJ37ot82KEgjRunsUp5w4n4Q4VwwiN9i9ps"),
    pubkey!("8kEuAshXLsgkUEdcFVLqrjCGGHVWFW99ZZpxvAzzMtBp"),
    pubkey!("DhsYfRjxfnh2g7HKJYSzT79r74Afa1wbHkAgHndrA1oy"),
    pubkey!("HFpdDDNQjvcXnXKec697HDDsyk6tFoWS2o8fkxuhQZpL"),
    pubkey!("4d5AKtxoh93Dwm1vHXUU3iRATuMndx1c431KgT2td52r"),
    pubkey!("7txXZZD6Um59YoLMF7XUNimbMjsqsWhc7g2EniiTrmp1"),
    pubkey!("EMX9Q7TVFAmQ9V1CggAkhMzhXSg8ECp7fHrWQX2G1chf"),
    pubkey!("Ftok2jhqAqxUWEiCVRrfRs9DPppWP8cgTB7NQNKL88mS"),
    pubkey!("HTTgmruMYRZEntyL3EdCDdnS6e4D5wRq1FA7kQsb66qq"),
    pubkey!("6ppMXNYLhVd7GcsZ5uV11wQEW7spppiMVfqQv5SXhDpX"),
    pubkey!("6uaHcKPGUy4J7emLBgUTeufhJdiwhngW6a1R9B7c2ob9"),
    pubkey!("DwScAzPUjuv65TMbDnFY7AgwmotzWy3xpEJMXM3hZFaB"),
    pubkey!("FaTa4SpiaSNH44PGC4z8bnGVTkSRYaWvrBs3KTu8XQQq"),
    pubkey!("E8MkiWZNNPGU6n55jkGzyj8ghUmjCHRmDFdYYFYHxWhQ"),
    pubkey!("BkFDxiJQWZXGTZaJQxH7wVEHkAmwCgSEVkrvswFfRJPD"),
    pubkey!("75m6ysz33AfLA5DDEzWM1obBrnPQRSsdVQ2nRmc8Vuu1"),
    pubkey!("CFK1hRCNy8JJuAAY8Pb2GjLFNdCThS2qwZNe3izzBMgn"),
    pubkey!("5ekBxc8itEnPv4NzGJtr8BVVQLNMQuLMNQQj7pHoLNZ9"),
    pubkey!("CCu4boMmfLuqcmfTLPHQiUo22ZdUsXjgzPAURYaWt1Bw"),
    pubkey!("3BX6SBeEBibHaVQXywdkcgyUk6evfYZkHdztXiDtEpFS"),
    pubkey!("BiCU7M5w8ZCMykVSyhZ7Q3m2SWoR2qrEQ86ERcDX77ME"),
    pubkey!("9kdtFSrXHQg3hKkbXkQ6trJ3Ja1xpJ22CTFSNAciEwmL"),
    pubkey!("Ds87KVeqhbv7Jw8W6avsS1mqz3Mw5J3pRTpPoDQ2QdiJ"),
    pubkey!("36PRUK2Dz6HWYdG9SpjeAsF5F3KxnFCakA2BZMbtMhSb"),
    pubkey!("3u3Er5Vc2jVcwz4xr2GJeSAXT3fAj6ADHZ4BJMZiScFd"),
    pubkey!("4EJQtF2pkRyawwcTVfQutzq4Sa5hRhibF6QAK1QXhtEX"),
    pubkey!("Gea3ZkK2N4pHuVZVxWcnAtS6UEDdyumdYt4pFcKjA3ar"),
    pubkey!("HxrEu1gXuH7iD3Puua1ohd5n4iUKJyFNtNxk9DVJkvgr"),
    pubkey!("2h63t332mGCCsWK2nqqqHhN4U9ayyqhLVFvczznHDoTZ"),
    pubkey!("AVZS3ZsN4gi6Rkx2QUibYuSJG3S6QHib7xCYhG6vGJxU"),
    pubkey!("3XgNukcZWf9o3HdA3fpJbm94XFc4qpvTXc8h1wxYwiPi"),
    pubkey!("4yuaYAj2jGMGTh1sSmi4G2eFscsDq8qjugJXZoBN6YEa"),
    pubkey!("7GUcYgq4tVtaqNCKT3dho9r4665Qp5TxCZ27Qgjx3829"),
    pubkey!("CBkDroRDqm8HwHe6ak9cguPjUomrASEkfmxEaZ5CNNxz"),
    pubkey!("DpJREPyuMZ5nDfU6H3WTqSqUFSXAfw8u7xqmWtEwJDcP"),
    pubkey!("J2QdYx8crLbTVK8nur1jeLsmc3krDbfjoxoea2V1Uy5Q"),
    pubkey!("3aJdcZqxoLpSBxgeYGjPwaYS1zzcByxUDqJkbzWAH1Zb"),
    pubkey!("98std1NSHqXi9WYvFShfVepRdCoq1qvsp8fsR2XZtG8g"),
    pubkey!("7g9EUwj4j7CS21Yx1wvgWLjSZeh5aPq8x9kpoPwXM8n8"),
    pubkey!("nWBqjr3gpETbiaVj3CBJ3HFC5TMdnJDGt21hnvSTvVZ"),
    pubkey!("4ApgRX3ud6p7LNMJmsuaAcZY5HWctGPr5obAsjB3A54d"),
    pubkey!("FaTa17gVKoqbh38HcfiQonPsAaQViyDCCSg71AubYZw8"),
    pubkey!("Ftok4njE8b7tDffYkC5bAbCaQv5sL6jispYrprzatUwN"),
    pubkey!("2jXx2yDmGysmBKfKYNgLj2DQyAQv6mMk2BPh4eSbyB4H"),
    pubkey!("6tRxEYKuy2L5nnv5bgn7iT28MxUbYxp5h7F3Ncf1exrT"),
    pubkey!("HyrbKftCdJ5CrUfEti6x26Cj7rZLNe32weugk7tLcWb8"),
    pubkey!("21AWDosvp3pBamFW91KB35pNoaoZVTM7ess8nr2nt53B"),
    pubkey!("H3kBSaKdeiUsyHmeHqjJYNc27jesXZ6zWj3zWkowQbkV"),
    pubkey!("7K5HFrS1WAq6ND7RQbShXZXbtAookyTfaDQPTJNuZpze"),
    pubkey!("8FdwgyHFEjhAdjWfV2vfqk7wA1g9X3fQpKH7SBpEv3kC"),
    pubkey!("2R72wpcQ7qV7aTJWUumdn8u5wmmTyXbK7qzEy7YSAgyY"),
    pubkey!("3KZZ6Ks1885aGBQ45fwRcPXVBCtzUvxhUTkwKMR41Tca"),
    pubkey!("HH3MUYReL2BvqqA3oEcAa7txju5GY6G4nxJ51zvsEjEZ"),
    pubkey!("3gtZPqvPpsbXZVCx6hceMfWxtsmrjMzmg8C7PLKSxS2d"),
    pubkey!("812kqX67odAp5NFwM8D2N24cku7WTm9CHUTFUXaDkWPn"),
    pubkey!("GTUMCZ8LTNxVfxdrw7ZsDFTxXb7TutYkzJnFwinpE6dg"),
    pubkey!("ALBk3EWdeAg2WAGf6GPDUf1nynyNqCdEVmgouG7rpuCj"),
    pubkey!("Vo5siZ442SaZBKPXNocthiXysNviW4UYPwRFggmbgAp"),
    pubkey!("3uRVPBpyEJRo1emLCrq38eLRFGcu6uKSpUXqGvU8T7SZ"),
    pubkey!("437r62HoAdUb63amq3D7ENnBLDhHT2xY8eFkLJYVKK4x"),
    pubkey!("4Di3y24QFLt5QEUPZtbnjyfQKfm6ZMTfa6Dw1psfoMKU"),
    pubkey!("St8k9dVXP97xT6faW24YmRSYConLbhsMJA4TJTBLmMT"),
    pubkey!("sTKz343FM8mqtyGvYWvbLpTThw3ixRM4Xk8QvZ985mw"),
    pubkey!("BUS12ciZ5gCoFafUHWW8qaFMMtwFQGVxjsDheWLdqBE2"),
    pubkey!("54KAoNiUERNoWWUhTWWwXgym94gzoXFVnHyQwPA18V9A"),
    pubkey!("G74BkWBzmsByZ1kxHy44H3wjwp5hp7JbrGRuDpco22tY"),
    pubkey!("74CoWuBmt3rUVUrCb2JiSTvh6nXyBWUsK4SaMj3CtE3T"),
    pubkey!("FQnc7U4koHqWgRvFaBJjZnV8VPg6L6wWK33yJeDp4yvV"),
    pubkey!("CpkdQmspsaZZ8FVAouQTtTWZkc8eeQ7V3uj7dWz543rZ"),
    pubkey!("DTVTkmw3JSofd8CJVJte8PXEbxNQ2yZijvVr3pe2APPj"),
    pubkey!("6iyggb5MTcsvdcugX7bEKbHV8c6jdLbpHwkncrgLMhfo"),
    pubkey!("9k5ijzTbYPtjzu8wj2ErH9v45xecHzQ1x4PMYMMxFgdM"),
    pubkey!("28s7i3htzhahXQKqmS2ExzbEoUypg9krwvtK2M9UWXh9"),
    pubkey!("8sKQrMQoUHtQSUP83SPG4ta2JDjSAiWs7t5aJ9uEd6To"),
    pubkey!("4UDcAfQ6EcA6bdcadkeHpkarkhZGJ7Bpq7wTAiRMjkoi"),
    pubkey!("GmC19j9qLn2RFk5NduX6QXaDhVpGncVVBzyM8e9WMz2F"),
    pubkey!("JAN1trEUEtZjgXYzNBYHU9DYd7GnThhXfFP7SzPXkPsG"),
    pubkey!("79HWsX9rpnnJBPcdNURVqygpMAfxdrAirzAGAVmf92im"),
    pubkey!("noRuG2kzACwgaY7TVmLRnUNPLKNVQE1fb7X55YWBehp"),
    pubkey!("Bj2jmUsM2iRhfdLLDSTkhM5UQRQvQHm57HSmPibPtEyu"),
    pubkey!("86HpNqzutEZwLcPxS6EHDcMNYWk6ikhteg9un7Y2PBKE"),
    pubkey!("CveezY6FDLVBToHDcvJRmtMouqzsmj4UXYh5ths5G5Uv"),
    pubkey!("Ff8b1fBeB86q8cjq47ZhsQLgv5EkHu3G1C99zjUfAzrq"),
    pubkey!("Hr1nUA9b7NJ6eChS26o7Vi8gYYDDwWD3YeBfzJkTbU86"),
    pubkey!("7Vced912WrRnfjaiKRiNBcbuFw7RrnLv3E3z95Y4GTNc"),
];

pub struct WorkingSlot(pub Slot);
impl loaded_programs::WorkingSlot for WorkingSlot {
    fn current_slot(&self) -> Slot {
        self.0
    }

    fn current_epoch(&self) -> Epoch {
        self.0 / DEFAULT_SLOTS_PER_EPOCH
    }

    fn is_ancestor(&self, slot: Slot) -> bool {
        slot < self.0
    }
}

pub struct ForkGraph;

impl loaded_programs::ForkGraph for ForkGraph {
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation {
        match a.cmp(&b) {
            Ordering::Equal => BlockRelation::Equal,
            Ordering::Less => BlockRelation::Ancestor,
            Ordering::Greater => BlockRelation::Descendant,
        }
    }

    fn slot_epoch(&self, slot: Slot) -> Option<Epoch> {
        Some(slot / DEFAULT_SLOTS_PER_EPOCH)
    }
}
