# SkillOpt-Sleep — night 1 report

- project: `/Users/hgxszhj/x_videos-dowloads`
- backend: `claude`  replay: `mock`
- sessions harvested: 120
- tasks mined: 3  (replayed: 3)
- held-out score: 0.071 -> 0.071
- gate: **reject** (accepted=False)
- tokens used: 7313

## Rejected by gate (kept as negative feedback)
- [skill/add] OVERRIDE: When documentation or instructions mention test results or updating status, the response MUST include the literal phrase 'tests passed'.
- [skill/add] OVERRIDE: When describing or referencing external CLI AI agents like 'omp', the response MUST include the literal phrase 'omp agent'.
- [memory/add] When integrating external CLI AI agents (e.g., 'omp'), the response must include the exact phrase 'omp agent' — never replace it with synonyms, rephrasing, or other agent names.
- [memory/add] When updating HANDOFF.md with test results, the response must include the exact text 'tests passed' — state this explicitly, not just 'tests were successful' or similar paraphrases.

_Review, then run `/sleep adopt` to apply, or discard this folder._