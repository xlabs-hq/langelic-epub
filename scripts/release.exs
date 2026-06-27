# Interactive release assistant. Run from the project root:
#
#     just release            # or:  elixir scripts/release.exs
#
# Shows the current and published versions, asks for a patch/minor/major bump,
# rolls the CHANGELOG, then (with your confirmation) commits, tags, and pushes —
# which starts the release workflow. The Hex publish still waits for your
# approval on the `hex` GitHub environment.
#
# Bumps BOTH mix.exs `@version` and native/langelic_epub/Cargo.toml `version`
# so they stay in lockstep (CI's bin/check_versions enforces this).
#
# Standalone Elixir — no mix/NIF compilation, just file edits + git.

defmodule Release do
  @cargo_toml "native/langelic_epub/Cargo.toml"

  def run do
    {app, current} = read_mix()
    ensure_cargo_in_sync!(current)
    branch = trimmed(git!(["rev-parse", "--abbrev-ref", "HEAD"]))

    info("package", app)
    info("current (mix.exs)", current)
    if v = published(app), do: info("latest on Hex", v)
    info("branch", branch)

    ensure_clean_tree!()
    confirm_branch!(branch)

    {maj, min, pat} = parse(current)

    choices = %{
      "1" => {"patch", "#{maj}.#{min}.#{pat + 1}", "bug fixes"},
      "2" => {"minor", "#{maj}.#{min + 1}.0", "new features, backwards-compatible"},
      "3" => {"major", "#{maj + 1}.0.0", "breaking changes"}
    }

    IO.puts("\nselect the release type:")

    for k <- ["1", "2", "3"] do
      {name, ver, note} = choices[k]
      IO.puts("  #{k}) #{name} → #{ver}\t(#{note})")
    end

    {_, new, _} = choices[prompt("choice [1-3]: ")] || abort()
    unless yes?("bump #{current} → #{new} ?"), do: abort()

    bump_mix!(current, new)
    bump_cargo!(current, new)
    roll_changelog!(current, new)

    IO.puts("\nchanges:")
    IO.puts(git!(["--no-pager", "diff", "--", "mix.exs", @cargo_toml, "CHANGELOG.md"]))

    unless yes?("commit, tag v#{new}, and push? (this starts the release build)") do
      IO.puts("""
      Edits left in place, uncommitted.
      Run `git checkout -- mix.exs #{@cargo_toml} CHANGELOG.md` to discard them.
      """)

      System.halt(0)
    end

    git!(["add", "mix.exs", @cargo_toml, "CHANGELOG.md"])
    git!(["commit", "-m", "Release #{new}"])
    git!(["tag", "-a", "v#{new}", "-m", "v#{new}"])
    git!(["push", "origin", branch])
    git!(["push", "origin", "v#{new}"])

    IO.puts("""

    ✅ pushed v#{new} — the release workflow is building the NIFs.
       Final step: approve the `hex` deployment to publish:
       #{actions_url()}
    """)
  end

  # ── mix.exs ────────────────────────────────────────────────────────────────
  defp read_mix do
    src = File.read!("mix.exs")
    [_, version] = Regex.run(~r/@version "([^"]+)"/, src) || die("no @version in mix.exs")
    [_, app] = Regex.run(~r/app:\s*:([a-z0-9_]+)/, src) || die("no `app:` in mix.exs")
    {app, version}
  end

  defp parse(version) do
    [maj, min, pat] =
      version
      |> String.split("-")
      |> hd()
      |> String.split(".")
      |> Enum.map(&String.to_integer/1)

    {maj, min, pat}
  end

  defp bump_mix!(old, new) do
    src = File.read!("mix.exs")
    File.write!("mix.exs", String.replace(src, ~s(@version "#{old}"), ~s(@version "#{new}")))
  end

  # ── Cargo.toml ───────────────────────────────────────────────────────────────
  # The crate version must match mix.exs (bin/check_versions gates this in CI).
  defp ensure_cargo_in_sync!(mix_version) do
    case cargo_version() do
      ^mix_version -> :ok
      other -> die("#{@cargo_toml} version (#{other}) != mix.exs (#{mix_version}) — fix before releasing.")
    end
  end

  defp cargo_version do
    src = File.read!(@cargo_toml)
    [_, v] = Regex.run(~r/^version\s*=\s*"([^"]+)"/m, src) || die("no version in #{@cargo_toml}")
    v
  end

  defp bump_cargo!(old, new) do
    src = File.read!(@cargo_toml)
    # Anchor on the package `version =` line (the first one) so feature/dep
    # version strings elsewhere in the file are untouched.
    bumped = Regex.replace(~r/^version\s*=\s*"#{Regex.escape(old)}"/m, src, ~s(version = "#{new}"), global: false)
    File.write!(@cargo_toml, bumped)
  end

  # ── CHANGELOG ──────────────────────────────────────────────────────────────
  # Opens a fresh `## [Unreleased]` section, stamps the dated `## [new]` heading,
  # and rolls the reference-style links at the bottom (Keep a Changelog style):
  # the Unreleased compare link is re-pointed at vNEW, and a vOLD...vNEW compare
  # link is added for the new version. Best-effort: if the links aren't in the
  # expected shape, only the heading is rolled.
  defp roll_changelog!(old, new) do
    path = "CHANGELOG.md"

    with true <- File.exists?(path),
         src = File.read!(path),
         true <- String.contains?(src, "## [Unreleased]") do
      today = Date.to_iso8601(Date.utc_today())
      heading = "## [Unreleased]\n\n## [#{new}] - #{today}"

      src
      |> String.replace("## [Unreleased]", heading, global: false)
      |> roll_changelog_links(old, new)
      |> then(&File.write!(path, &1))
    else
      _ -> :ok
    end
  end

  defp roll_changelog_links(src, old, new) do
    case Regex.run(~r{^\[Unreleased\]:\s*(\S+?)/compare/v#{Regex.escape(old)}\.\.\.HEAD\s*$}m, src) do
      [line, base] ->
        replacement =
          "[Unreleased]: #{base}/compare/v#{new}...HEAD\n" <>
            "[#{new}]: #{base}/compare/v#{old}...v#{new}"

        String.replace(src, line, replacement, global: false)

      _ ->
        src
    end
  end

  # ── Hex (best-effort) ──────────────────────────────────────────────────────
  defp published(app) do
    case System.cmd("mix", ["hex.info", to_string(app)], stderr_to_stdout: true) do
      {out, 0} -> Regex.run(~r/[0-9]+\.[0-9]+\.[0-9]+/, out) |> then(&(&1 && hd(&1)))
      _ -> nil
    end
  rescue
    _ -> nil
  end

  # ── git ────────────────────────────────────────────────────────────────────
  defp ensure_clean_tree! do
    case trimmed(git!(["status", "--porcelain"])) do
      "" -> :ok
      _ -> die("working tree is dirty — commit or stash first.")
    end
  end

  defp confirm_branch!(b) when b in ["master", "main"], do: :ok

  defp confirm_branch!(b) do
    unless yes?("⚠  not on master/main (on '#{b}'). release from here anyway?"), do: abort()
  end

  defp git!(args) do
    case System.cmd("git", args, stderr_to_stdout: true) do
      {out, 0} -> out
      {out, code} -> die("git #{Enum.join(args, " ")} failed (#{code}):\n#{out}")
    end
  end

  defp actions_url do
    case System.cmd("git", ["config", "--get", "remote.origin.url"]) do
      {url, 0} ->
        slug =
          url
          |> String.trim()
          |> String.replace(~r/\.git$/, "")
          |> String.replace(~r{^git@github\.com:}, "")
          |> String.replace(~r{^https://github\.com/}, "")

        "https://github.com/#{slug}/actions"

      _ ->
        "your repo's Actions tab"
    end
  end

  # ── IO ─────────────────────────────────────────────────────────────────────
  defp info(label, value), do: IO.puts(String.pad_trailing("#{label}:", 19) <> value)
  defp prompt(label), do: IO.gets(label) |> to_string() |> String.trim()
  defp yes?(question), do: prompt("#{question} [y/N] ") =~ ~r/^[Yy]/
  defp trimmed(s), do: String.trim(s)
  defp abort, do: die("aborted.")

  defp die(msg) do
    IO.puts(:stderr, "✗ #{msg}")
    System.halt(1)
  end
end

Release.run()
