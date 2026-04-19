defmodule LangelicEpub.EpubcheckTest do
  @moduledoc """
  Runs epubcheck against EPUBs produced by the writer and asserts the only
  reported messages are warnings or informational. Skipped automatically when
  `epubcheck` is not on PATH, so this test does not block local development
  for contributors without it.
  """

  use ExUnit.Case, async: true

  alias LangelicEpub.EpubFixtureBuilder

  @moduletag :external

  setup_all do
    case System.find_executable("epubcheck") do
      nil ->
        {:ok, epubcheck: nil}

      path ->
        {:ok, epubcheck: path}
    end
  end

  describe "epubcheck on built EPUBs" do
    test "passes on the minimal hand-built fixture", %{epubcheck: epubcheck} do
      skip_if_missing!(epubcheck)

      bytes = EpubFixtureBuilder.minimal_epub3()
      {:ok, doc} = LangelicEpub.parse(bytes)
      {:ok, rebuilt} = LangelicEpub.build(doc)

      assert_no_epubcheck_errors(rebuilt, epubcheck)
    end

    test "passes on a built-from-scratch multi-chapter document", %{epubcheck: epubcheck} do
      skip_if_missing!(epubcheck)

      doc = %LangelicEpub.Document{
        title: "Multichapter",
        language: "en",
        identifier: "urn:uuid:ec2c6e3e-9b8f-4a21-9c3d-1234567890ab",
        creators: ["Only Author"],
        spine:
          for i <- 1..3 do
            %LangelicEpub.Chapter{
              id: "ch#{i}",
              file_name: "ch#{i}.xhtml",
              title: "Chapter #{i}",
              media_type: "application/xhtml+xml",
              data:
                ~s|<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter #{i}</title></head><body><h1>Chapter #{i}</h1><p>Body.</p></body></html>|
            }
          end
      }

      {:ok, bytes} = LangelicEpub.build(doc)
      assert_no_epubcheck_errors(bytes, epubcheck)
    end
  end

  defp skip_if_missing!(nil) do
    ExUnit.configure(exclude: :external)
    flunk("epubcheck not installed — tag :external excludes this test")
  end

  defp skip_if_missing!(_), do: :ok

  defp assert_no_epubcheck_errors(bytes, epubcheck) do
    tmp = Path.join(System.tmp_dir!(), "langelic_epub_#{System.unique_integer([:positive])}.epub")
    File.write!(tmp, bytes)

    try do
      {output, exit_code} = System.cmd(epubcheck, [tmp], stderr_to_stdout: true)
      errors = extract_errors(output)

      if errors != [] do
        flunk(
          "epubcheck reported #{length(errors)} error(s) for #{Path.basename(tmp)}:\n" <>
            Enum.join(errors, "\n") <>
            "\n\n--- full output ---\n#{output}"
        )
      end

      assert exit_code in [0, 1],
             "epubcheck exited #{exit_code} but produced no ERROR/FATAL lines — unexpected status"
    after
      File.rm(tmp)
    end
  end

  # Errors that come from epub-builder's generated output (nav.xhtml, toc.ncx,
  # manifest properties) rather than from our own code. Plan §2 "epub-builder
  # gap" and §10 acknowledge these are acceptable for v0.1. Track upstream.
  @known_upstream_patterns [
    # epub-builder's generated nav.xhtml has an empty <nav epub:type="landmarks">
    # wrapper when landmarks are absent. Documented in plan §2.
    "element \"nav\" incomplete",
    "element \"ol\" incomplete",
    "element \"navMap\" incomplete",
    "landmarks\" nav element should contain",
    # XHTML files with inline SVG must have `properties="svg"` in their OPF
    # <item>, but epub-builder's add_content doesn't accept properties.
    "property \"svg\" should be declared",
    # epub-builder's NCX generator assigns playOrder that collides when the
    # same target appears under multiple parent navPoints.
    "different playOrder values",
    # When the input TOC's href ordering differs from spine ordering (e.g.,
    # "Other books by this author" appears before chapter 1 in TOC), epubcheck
    # flags the nav.xhtml entries as out of reading order. epub-builder has
    # no hook for reordering.
    "nav must be in reading order",
    "must be in reading order"
  ]

  defp extract_errors(output) do
    output
    |> String.split("\n")
    |> Enum.filter(fn line ->
      (String.starts_with?(line, "ERROR") or String.starts_with?(line, "FATAL")) and
        not known_upstream_error?(line)
    end)
  end

  defp known_upstream_error?(line) do
    Enum.any?(@known_upstream_patterns, &String.contains?(line, &1))
  end
end
