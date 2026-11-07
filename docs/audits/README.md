# Audits

Every change to jaw's data layer is audited by the database expert (`.cursor/agents/database-expert.md`) before it lands, against "Data integrity" and "Migrations" in the design. Each audit is a note here, `YYYY-MM-DD-<topic>.md`, ending in `Verdict: approved` or `Verdict: changes needed`.

Every commit that touches the data layer carries the trailer `Data-Audit: docs/audits/<file>.md`, naming an approved note. `scripts/check-data-audit.py` (`mise run check-audit`, and CI) checks it, and lists the paths that count as the data layer.

The review that set the rules is `docs/reviews/2026-10-16-data-review.md`.
