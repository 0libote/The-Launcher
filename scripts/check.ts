// Keep the same checks on Windows and Unix; no shell-specific command chaining.
for (const args of [
  ["fmt", "--all", "--", "--check"],
  ["clippy", "--all-targets", "--", "-D", "warnings"],
  ["test", "--all-targets"],
]) {
  const result = Bun.spawnSync(["cargo", ...args], { stdout: "inherit", stderr: "inherit" });
  if (result.exitCode !== 0) process.exit(result.exitCode);
}
