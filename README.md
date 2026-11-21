# AMM Solana

A constant-product automated market maker (AMM) built with Solana and Anchor.

## Assignment scope

- [ ] Implement the AMM program
- [ ] Add swap fees and treasury accounting
- [ ] Test every instruction
- [ ] Optional: implement the CPMM math without an external library
- [ ] Document DeFi downtime mitigation

## Design

The pool follows the constant-product invariant:

```
x * y = k
```

The CPMM arithmetic will be implemented locally using checked integer operations and u128 intermediate values. Token quantities are raw base units; token decimals are a client/display concern.

Initial design choices:
- Constant-product market maker.
- 30 basis-point (0.30%) initial swap fee.
- Explicit treasury accounting, separate from LP reserves.
- Anchor program with SPL Token accounts.
- LiteSVM-based integration tests.

## Planned workspace

```
programs/amm/src/
  lib.rs
  constants.rs
  error.rs
  state.rs
  math/
    mod.rs
    curve.rs
    liquidity.rs
  instructions/
    initialize.rs
    deposit.rs
    swap.rs
    withdraw.rs
    withdraw_fees.rs
    update_config.rs
```

## Development

Once the Anchor workspace scaffold is committed:

```bash
anchor build
anchor test
```

## Downtime resilience

The application will document RPC failover, bounded transaction retries, frontend redundancy, monitoring/alerting, indexer rebuildability from canonical chain state, and a narrowly authorized emergency pause.

## Security

Educational project, not audited and not suitable for real funds. Production deployment requires independent security review, adversarial tests, careful PDA/token-account validation, and operational controls.
