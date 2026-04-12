<div align="center">

# WWC
### World Wide Currency

**A blockchain where every token represents real, verified machine work.**
Built on Substrate. Deploy and abandon. Zero human control.

![Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)
![Substrate](https://img.shields.io/badge/Framework-Substrate-blueviolet.svg)
![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)

</div>

---

## What is WWC?

WWC (World Wide Currency) is a sovereign blockchain built on Substrate
where tokens are minted exclusively through verified computational work.
No human can create, pause, or destroy tokens. The rules are immutable.

Every WWC token exists because a machine somewhere in the world contributed
real work to improve [AURA](https://github.com/Soflution1/aura), the
community-driven self-improving AI.

---

## Core principles

**Unlimited supply, zero human control.** WWC has no hard cap like Bitcoin.
New tokens are minted algorithmically when verified contributions are validated
by consensus. The supply grows with the network's real output.

**Deploy and abandon.** Once the genesis block is live, all admin keys are
destroyed. No one can modify the emission rules. Not the founders, not a
foundation, not a government. The code is the law.

**Proof of contribution.** Unlike Bitcoin's proof of work (which wastes energy
on arbitrary puzzles), WWC rewards actual useful work: LoRA fine-tuning
improvements to the AURA model, validated by benchmark consensus.

---

## Token emission rules (immutable after genesis)

| Contribution | Reward |
|---|---|
| LoRA improvement validated (benchmark +1% minimum) | 100 WWC |
| Benchmark submitted and validated | 10 WWC |
| Active validator node (per day) | 50 WWC |
| Any other source | 0 WWC (impossible) |

No mint function. No owner. No multisig. No pause. No backdoor.

---

## Initial distribution (before deploy and abandon)

Total genesis supply: 1,000,000,000 WWC

| Allocation | Share | Vesting |
|---|---|---|
| Contributors (via smart contract) | 40% | Distributed on contribution |
| Foundation (future development) | 20% | Governed by on-chain votes |
| Soflution / founders | 15% | 4-year linear vesting |
| Network validators | 15% | Daily rewards |
| Initial public sale | 10% | Unlocked at genesis |

After genesis: unlimited algorithmic emission only.

---

## Architecture

```
WWC Chain (Substrate)
├── pallets/wwc-token    — Token logic, minting, immutable rules
├── runtime              — Chain runtime (upgradable via governance)
├── node                 — Network node implementation
└── scripts              — Tools and utilities
```

---

## Built with

- **[Substrate](https://github.com/paritytech/polkadot-sdk)** — Blockchain framework by Parity Technologies
- **Rust** — Systems programming language
- **AURA** — The AI model that generates contributions

---

## License

Apache 2.0. Use it, fork it, build on it. No restrictions.
