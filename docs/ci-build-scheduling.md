# Parallel native plugin builds

Each OS builds its own target; there is no cross-compilation of the other platforms.

| Shard | Plugins |
| --- | --- |
| Office | office-preview |
| Data | data-preview |
| Services | ssh, git, webdav, gdrive, smb, openlist, archive, checksum, everything, gpg, email |

All three shards run concurrently on Linux, Windows and macOS (nine native jobs, subject to GitHub runner capacity). Cargo controls CPU parallelism within each job. Format checks run once before native jobs. Panel builds run alongside native compilation, then each platform assembles the shared panels with its own tested native binaries.

Every plugin retains unit tests, Clippy and release compilation on each OS. Process handshakes run per shard. Services retain Git, SSH, Google Drive and connection-sync regressions, plus the beginner example builds. Git's search fixture supplies the required `mode: name` field. This fixes an observed CI fixture failure without weakening the assertion.

Native cache keys include target and shard, and cache discovery uses the actual root Cargo workspace/lockfile. Raw binary artifacts are tar archives, preserving executable permissions between runners. Packages keep their existing names and catalog format; artifacts are not recompressed by GitHub after packaging.

Linux Core and remote integrations depend only on the Linux packages. They no longer wait for Windows/macOS. Core examples and integration examples share one Cargo target directory, and both integration jobs share a cache. Release still requires all three platform packages and both integration jobs. Failures remain strict and outdated PR runs are cancelled.

The observed macOS run spent about 13 minutes on Office release compilation alone, so isolating Office shortens the serial critical path but does not remove its own compile time. More runners and duplicated dependency work can increase total compute minutes. No local builds or test suites were run for this change, as requested; new CI timings and native artifact acceptance remain pending.
