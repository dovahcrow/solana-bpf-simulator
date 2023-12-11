# Solana BPF Simulator
Simulate a solana program locally without uploading it.

## Example
Download the corresponding binary from the releases first.

Get the program data:
```shell
solana-bpf-simulator get-program-data --program GFXsSL5sSaDfNFQUYsHekbWBW1TsFdjDYzACh62tEHxn
```
Or prepare the program data (e.g. the *.so from cargo build-sbf) by yourself.

Then:
```shell
solana-bpf-simulator simulate --program program.so --program-id GFXsSL5sSaDfNFQUYsHekbWBW1TsFdjDYzACh62tEHxn --instruction "fwjdYzA77n1" --account F451mjRqGEu1azbj46v4FuMEt1CacaPHQKUHzuTqKp4R --account 3NvDNxLa1AZofqmsfxRLswWUh1QqoWNQxm2C7azeaKFt --account 294A1PmDuLHrmgcPKHoQd9GgrJxQ6y2WkpPhSVgbkUoD --account Af6DjX1eRjfmnF1Lfe99FTgLUMKTVrsyf7CY2Kx9kzbT --account 7T9Y3aBkfUWNAocUijEJkzGBVG9CvawyqU7PuHE5RuDp --account oQZvi1vixbxer5XNrGHjwSHsrVeu3FFekw2oaV9DCGy --account 5Z2m2Hyc6S1Hmuee5HPnh2gWVGf4cHgFtzJWnRBV4zE7 --account H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG --account 5Yk3rUpMK2fANTkZtcD7dcHjsHC7KSBXLjDjvr3pKZsd --account 7yyaeuJ1GGtVBLT2z2xub5ZWYKaNhF28mj1RdV4VDFVk --account FdUm8MtCFGMC2UvxEV2bywKBQaP6es7osMwqZ9i2Gbvi --account 6kCU7GxqqUwuPMSrcXq5sXicoXqnA6KnsavzGcbZzRcg --account EoApD8hkDePGpfMwA9rpwwUGJuMZ7u2wK6z9z2pk7EoM --account TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA --writable-account 3NvDNxLa1AZofqmsfxRLswWUh1QqoWNQxm2C7azeaKFt --writable-account 294A1PmDuLHrmgcPKHoQd9GgrJxQ6y2WkpPhSVgbkUoD --writable-account Af6DjX1eRjfmnF1Lfe99FTgLUMKTVrsyf7CY2Kx9kzbT --writable-account 7T9Y3aBkfUWNAocUijEJkzGBVG9CvawyqU7PuHE5RuDp --writable-account oQZvi1vixbxer5XNrGHjwSHsrVeu3FFekw2oaV9DCGy --writable-account 5Z2m2Hyc6S1Hmuee5HPnh2gWVGf4cHgFtzJWnRBV4zE7 --writable-account 5Yk3rUpMK2fANTkZtcD7dcHjsHC7KSBXLjDjvr3pKZsd --writable-account EoApD8hkDePGpfMwA9rpwwUGJuMZ7u2wK6z9z2pk7EoM
```

## Note

* The instruction data are base58 encoded.
* Specify each account needed using `--account`.
* Specify each writable account again using `--writable-account`.
* Specify the signer using `--signer-account`.
