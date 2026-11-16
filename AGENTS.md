# OnSet continuation

Start with [docs/HANDOFF.md](docs/HANDOFF.md). The requested product specification is [docs/PROJECT_PLAN.md](docs/PROJECT_PLAN.md); [docs/PLAN_PROGRESS.md](docs/PLAN_PROGRESS.md) maps baseline implementation and open verification. Read the requirements relevant to a change and update evidence/status when completing work.

OnSet is one Windows app for DJs: preparation (the Python service in `service/` and the screens in `ui/`) and live visuals (the Rust crates in `crates/`). The name "autocue" is reserved for, at most, the code that places cues automatically; do not reintroduce it as a product, folder or module name. Preserve the DJ's control, local audio defaults, human corrections, explicit cue approvals, exact write previews and backup/restore behavior. Generated fixtures and model execution are engineering evidence, not listening ground truth or real rekordbox acceptance. Do not mark the full plan complete while the documented gates remain open.

The working tree contains extensive existing modified/untracked implementation. Preserve it. Music, credentials, private reports, runtime/model caches and local databases must stay excluded from source tracking. docs/HANDOFF.md records current build identity, test commands and Windows AppData virtualization precautions. Older September documents are historical; use the current handoff and runbook.
