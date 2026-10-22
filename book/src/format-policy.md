# Formats we read

UnENVerse reads the current vault, backup and package formats and the previous two. A format older than that is read by an earlier release, which the release notes name; the golden fixtures in `tests/fixtures/` pin what each reader accepts. Dropping a format is announced in the changelog one release before.
