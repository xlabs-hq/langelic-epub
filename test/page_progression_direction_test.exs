defmodule LangelicEpub.PageProgressionDirectionTest do
  use ExUnit.Case, async: true

  alias LangelicEpub.Chapter
  alias LangelicEpub.Document
  alias LangelicEpub.EpubFixtureBuilder
  alias LangelicEpub.Error

  defp base_doc(opts) do
    %Document{
      title: "Direction Test",
      identifier: "urn:uuid:ppd-test",
      language: Keyword.get(opts, :language, "ar"),
      creators: ["Tester"],
      page_progression_direction: Keyword.get(opts, :page_progression_direction, nil),
      spine: [
        %Chapter{
          id: "ch1",
          file_name: "ch1.xhtml",
          title: "One",
          media_type: "application/xhtml+xml",
          data:
            ~s|<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body><p>x</p></body></html>|
        }
      ]
    }
  end

  # Unzip built EPUB bytes and return {opf_string, nav_string}. epub-builder
  # writes both under OEBPS/.
  defp opf_and_nav(bytes) do
    {:ok, handle} = :zip.zip_open(bytes, [:memory])
    {:ok, {_, opf}} = :zip.zip_get(~c"OEBPS/content.opf", handle)
    {:ok, {_, nav}} = :zip.zip_get(~c"OEBPS/nav.xhtml", handle)
    :zip.zip_close(handle)
    {to_string(opf), to_string(nav)}
  end

  # Isolate the <spine ...> opening tag from an OPF string.
  defp spine_tag(opf) do
    [_, rest] = String.split(opf, "<spine", parts: 2)
    [attrs | _] = String.split(rest, ">", parts: 2)
    "<spine" <> attrs <> ">"
  end

  # Isolate the root <html ...> opening tag from an XHTML string.
  defp html_tag(xhtml) do
    [_, rest] = String.split(xhtml, "<html", parts: 2)
    [attrs | _] = String.split(rest, ">", parts: 2)
    "<html" <> attrs <> ">"
  end

  describe "rtl" do
    test "spine carries page-progression-direction=\"rtl\"" do
      {:ok, bytes} = LangelicEpub.build(base_doc(page_progression_direction: "rtl"))
      {opf, _nav} = opf_and_nav(bytes)

      assert spine_tag(opf) =~ ~s|page-progression-direction="rtl"|
    end

    test "nav.xhtml root <html> gets dir=\"rtl\" and the document language" do
      {:ok, bytes} =
        LangelicEpub.build(base_doc(page_progression_direction: "rtl", language: "ar"))

      {_opf, nav} = opf_and_nav(bytes)
      tag = html_tag(nav)

      assert tag =~ ~s|dir="rtl"|
      assert tag =~ ~s|xml:lang="ar"|
      assert tag =~ ~s|lang="ar"|
    end
  end

  describe "ltr" do
    test "spine carries page-progression-direction=\"ltr\" and nav is not oriented" do
      {:ok, bytes} =
        LangelicEpub.build(base_doc(page_progression_direction: "ltr", language: "en"))

      {opf, nav} = opf_and_nav(bytes)

      assert spine_tag(opf) =~ ~s|page-progression-direction="ltr"|
      # dir is only applied for rtl.
      refute html_tag(nav) =~ ~s|dir=|
    end
  end

  describe "nil (default)" do
    test "spine omits the page-progression-direction attribute" do
      {:ok, bytes} = LangelicEpub.build(base_doc(page_progression_direction: nil))
      {opf, nav} = opf_and_nav(bytes)

      refute spine_tag(opf) =~ "page-progression-direction"
      refute html_tag(nav) =~ ~s|dir=|
    end
  end

  describe "validation" do
    test "an unrecognised value fails the build" do
      assert {:error, %Error{kind: :invalid_page_direction, message: msg}} =
               LangelicEpub.build(base_doc(page_progression_direction: "sideways"))

      assert msg =~ "sideways"
    end

    test "case-sensitive: uppercase RTL is rejected" do
      assert {:error, %Error{kind: :invalid_page_direction}} =
               LangelicEpub.build(base_doc(page_progression_direction: "RTL"))
    end
  end

  describe "no round-trip from source" do
    test "parsing an rtl-built EPUB still succeeds and reports nil direction" do
      {:ok, bytes} =
        LangelicEpub.build(base_doc(page_progression_direction: "rtl", language: "ar"))

      {:ok, parsed} = LangelicEpub.parse(bytes)

      # Direction is a build-time, target-language decision — the parser
      # deliberately does not surface the source EPUB's spine direction.
      assert parsed.page_progression_direction == nil
      assert parsed.title == "Direction Test"
      assert length(parsed.spine) == 1
    end

    test "the default struct leaves page_progression_direction nil" do
      assert %Document{}.page_progression_direction == nil
    end

    test "a parsed hand-built fixture reports nil direction" do
      {:ok, parsed} = LangelicEpub.parse(EpubFixtureBuilder.minimal_epub3())
      assert parsed.page_progression_direction == nil
    end
  end
end
