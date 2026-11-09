# Testnet smoke test

Run on 2026-11-07 against the contract in `deployments/testnet.json` (the post-audit build), native XLM
Stellar Asset Contract as the token, fee 100 bps, `sp-*` testnet identities. `settle_mode 1` is `Manual`.
Output lines are truncated.

Result: all flows succeeded, and the two audit fixes were checked live:

- `initialize` with `reclaim_delay 0` is rejected with contract error #14 (`InvalidDelay`).
- After a payment is created, the admin raised the fee to 500 bps before the payer paid. The payment still settled
  at its create-time 100 bps: merchant +9,900,000, fee collector +100,000.
- create -> cancel and create -> pay -> refund (full amount back to the payer) succeeded.

An earlier build (superseded, see `deployments/testnet.json`) passed the same flows on 2026-11-06.

```
## initialize with out-of-range reclaim_delay is rejected (audit H1)
$ stellar contract invoke --id stellar-pay-core --source-account sp-admin -- initialize --admin GAFXT4ZSQ3URW2YQSVTJEXXMFR5G5IABR4RVPXS66VH5TEOPURA3RMJB --token CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC --fee_bps 100 --fee_recipient GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ --reclaim_delay 0
❌ error: transaction simulation failed: HostError: Error(Contract, #14)

Event log (newest first):
   0: [Diagnostic Event] contract:CDB5PL5RITGAI6FBYTICPZMQIFROZEMXYMSKFGVK5NOUEX2XLYQ3IS4I, topics:[error, Error(Contract, #14)], data:"escalating Ok(ScErrorType::Contract) frame-exit to Err"
   1: [Diagnostic Event] topics:[fn_call, CDB5PL5RITGAI6FBYTICPZMQIFROZEMXYMSKFGVK5NOUEX2XLYQ3IS4I, initialize], data:[GAFXT4ZSQ3URW2YQSVTJEXXMFR5G5IABR4RVPXS66VH5TEOPURA3RMJB, CDLZFC3SYJYDZT7K67VZ75H

## initialize (valid)
$ stellar contract invoke --id stellar-pay-core --source-account sp-admin -- initialize --admin GAFXT4ZSQ3URW2YQSVTJEXXMFR5G5IABR4RVPXS66VH5TEOPURA3RMJB --token CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC --fee_bps 100 --fee_recipient GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ --reclaim_delay 3600
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-admin -- get_config
{"admin":"GAFXT4ZSQ3URW2YQSVTJEXXMFR5G5IABR4RVPXS66VH5TEOPURA3RMJB","fee_bps":100,"fee_recipient":"GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ","paused":false,"reclaim_delay":3600,"token"
## before
balance merchant: "100009900000"
balance feecollector: "100000100000"
## create(manual) -> admin raises fee to 5% -> pay -> settle (fee must stay 1%, audit M4)
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- create_payment --operator GBD4O2LPIFSN2Y4PHNFCHNKGKBXVFKYD6B6SEBXFSW4AEO5T6DUOH4ZX --merchant GBQALWO42IYR46ANUJXSHW4VZ2VHNG6BOCIZPOTPUPWFSTIOZORGV7DU --payment_id 1111111111111111111111111111111111111111111111111111111111111111 --amount 10000000 --reference abababababababababababababababababababababababababababababababab --settle_mode 1
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-admin -- set_fee --fee_bps 500 --fee_recipient GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-payer -- pay --payer GDMGBGM5CPD6KSDUKWCH4RTN4J4PC2W57LZ36TNEWEARJXI57KC2SN6S --payment_id 1111111111111111111111111111111111111111111111111111111111111111
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- settle --payment_id 1111111111111111111111111111111111111111111111111111111111111111
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-admin -- set_fee --fee_bps 100 --fee_recipient GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ
✅ Transaction submitted successfully!
null
## after settle (expect merchant +9900000, fee +100000)
balance merchant: "100019800000"
balance feecollector: "100000200000"
## create -> cancel
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- create_payment --operator GBD4O2LPIFSN2Y4PHNFCHNKGKBXVFKYD6B6SEBXFSW4AEO5T6DUOH4ZX --merchant GBQALWO42IYR46ANUJXSHW4VZ2VHNG6BOCIZPOTPUPWFSTIOZORGV7DU --payment_id 1212121212121212121212121212121212121212121212121212121212121212 --amount 10000000 --reference abababababababababababababababababababababababababababababababab --settle_mode 1
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- cancel --payment_id 1212121212121212121212121212121212121212121212121212121212121212
✅ Transaction submitted successfully!
null
## create -> pay -> refund
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- create_payment --operator GBD4O2LPIFSN2Y4PHNFCHNKGKBXVFKYD6B6SEBXFSW4AEO5T6DUOH4ZX --merchant GBQALWO42IYR46ANUJXSHW4VZ2VHNG6BOCIZPOTPUPWFSTIOZORGV7DU --payment_id 1313131313131313131313131313131313131313131313131313131313131313 --amount 10000000 --reference abababababababababababababababababababababababababababababababab --settle_mode 1
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-payer -- pay --payer GDMGBGM5CPD6KSDUKWCH4RTN4J4PC2W57LZ36TNEWEARJXI57KC2SN6S --payment_id 1313131313131313131313131313131313131313131313131313131313131313
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- refund --payment_id 1313131313131313131313131313131313131313131313131313131313131313
✅ Transaction submitted successfully!
null
$ stellar contract invoke --id stellar-pay-core --source-account sp-operator -- get_payment --payment_id 1313131313131313131313131313131313131313131313131313131313131313
{"amount":"10000000","closed_at":1794038491,"condition":0,"created_at":1794038436,"fee_bps":100,"fee_recipient":"GDWOAGPIC3QVJKGOXGGQEWXRTUMQ6N2P7EWAE4FS6RSWOUUYZXPMNMPQ","merchant":"GBQALWO42IYR46ANU
```
