# Weekly sync, 2026-11-12

Attendees: Ana, Jörg, Mei

## Agenda

1. Release 0.3 status
2. Open bugs
3. Next steps

## Notes

Release is **on track**. The importer still fails for files > 2 GB, see [issue 214](https://example.com/issues/214 "Importer fails on large files").

- Ana: finish the *round-trip* tests
  - corpus needs more edge cases
  - CRLF files!
- Jörg: review `parse.rs`
- Mei: write docs

## Decisions

| Topic      | Owner | Due        |
| ---------- | :---: | ---------: |
| Importer   | Ana   | 2026-11-20 |
| Docs       | Mei   | 2026-11-27 |
| ~~Legacy~~ | –     | dropped    |

> Ship it when it is ready, not before.

Next meeting: [[2026-11-19 Weekly sync]]
