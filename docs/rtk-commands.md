# RTK Commands

**RTK** = Rust Token Killer - 60-90% token 节省

## Golden Rule

**Always prefix commands with `rtk`**: `rtk git status`

## Build & Compile (80-90% savings)
```bash
rtk cargo build     # Cargo build output
rtk cargo check     # Cargo check output
rtk cargo clippy    # Clippy warnings grouped by file
```

## Test (60-99% savings)
```bash
rtk cargo test     # Cargo test failures only
rtk go test        # Go test failures only
rtk jest           # Jest failures only
rtk pytest         # Python test failures only
```

## Git (59-80% savings)
```bash
rtk git status     # Compact status
rtk git log        # Compact log
rtk git diff       # Compact diff
rtk git add        # Ultra-compact confirmations
rtk git commit     # Ultra-compact confirmations
rtk git push       # Ultra-compact confirmations
```

## GitHub (26-87% savings)
```bash
rtk gh pr view <num>   # Compact PR view
rtk gh pr checks       # Compact PR checks
rtk gh issue list     # Compact issue list
```

## Package Managers (70-90% savings)
```bash
rtk pnpm list         # Compact dependency tree
rtk pnpm outdated     # Compact outdated packages
```

## Files & Search (60-75% savings)
```bash
rtk ls <path>        # Tree format, compact
rtk grep <pattern>   # Search grouped by file
rtk find <pattern>   # Find grouped by directory
```

## Meta Commands
```bash
rtk gain            # View token savings statistics
rtk gain --history  # View command history with savings
rtk proxy <cmd>     # Run command without filtering
```

## Token Savings Overview

| Category | Savings |
|----------|---------|
| Tests | 90-99% |
| Build | 70-87% |
| Git | 59-80% |
| Package Managers | 70-90% |

**Overall average: 60-90% token reduction**
