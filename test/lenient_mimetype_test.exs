defmodule LangelicEpub.LenientMimetypeTest do
  @moduledoc """
  Real-world EPUBs (Calibre, fan exporters, several conversion tools) often
  emit a `mimetype` file that is _almost_ but not exactly the canonical
  `application/epub+zip` — typically with a trailing LF, CRLF, or space, and
  occasionally a leading UTF-8 BOM. Apple Books, Calibre, and ADE accept
  these silently; rejecting them would orphan a long tail of otherwise-valid
  documents. See `docs/2026-04-26-mimetype-trailing-whitespace.md`.
  """

  use ExUnit.Case, async: true

  alias LangelicEpub.{Chapter, Document, EpubFixtureBuilder, Error}

  describe "lenient mimetype acceptance" do
    test "trailing LF (Calibre / Blindsight case)" do
      bytes = EpubFixtureBuilder.minimal_epub3(mimetype: "application/epub+zip\n")

      assert {:ok, %Document{spine: [%Chapter{}]} = doc} = LangelicEpub.parse(bytes)
      assert doc.title == "Minimal EPUB 3"
    end

    test "trailing CRLF (Windows-line-ending exporters)" do
      bytes = EpubFixtureBuilder.minimal_epub3(mimetype: "application/epub+zip\r\n")

      assert {:ok, %Document{spine: [%Chapter{}]}} = LangelicEpub.parse(bytes)
    end

    test "trailing space" do
      bytes = EpubFixtureBuilder.minimal_epub3(mimetype: "application/epub+zip ")

      assert {:ok, %Document{spine: [%Chapter{}]}} = LangelicEpub.parse(bytes)
    end

    test "leading UTF-8 BOM" do
      bytes =
        EpubFixtureBuilder.minimal_epub3(mimetype: <<0xEF, 0xBB, 0xBF>> <> "application/epub+zip")

      assert {:ok, %Document{spine: [%Chapter{}]}} = LangelicEpub.parse(bytes)
    end
  end

  describe "regression: genuine non-EPUB content still rejected" do
    test "wrong mimetype string returns :invalid_mimetype" do
      bytes = EpubFixtureBuilder.minimal_epub3(mimetype: "application/zip")

      assert {:error, %Error{kind: :invalid_mimetype, message: msg}} =
               LangelicEpub.parse(bytes)

      assert msg =~ "application/epub+zip"
      assert msg =~ "application/zip"
    end

    test "random bytes for the mimetype return :invalid_mimetype" do
      bytes =
        EpubFixtureBuilder.minimal_epub3(mimetype: <<0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE>>)

      assert {:error, %Error{kind: :invalid_mimetype}} = LangelicEpub.parse(bytes)
    end
  end
end
