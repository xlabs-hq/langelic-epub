defmodule LangelicEpubTest do
  use ExUnit.Case, async: true

  alias LangelicEpub.{Asset, Chapter, Document, EpubFixtureBuilder, Error, NavItem}

  describe "parse/1 on a minimal hand-built EPUB 3" do
    setup do
      bytes =
        EpubFixtureBuilder.minimal_epub3(
          title: "Minimal EPUB 3",
          language: "en",
          identifier: "urn:uuid:minimal-epub-3",
          creators: ["Jane Doe"],
          rights: "CC0 1.0"
        )

      {:ok, bytes: bytes}
    end

    test "returns a Document with core metadata populated", %{bytes: bytes} do
      assert {:ok, %Document{} = doc} = LangelicEpub.parse(bytes)
      assert doc.title == "Minimal EPUB 3"
      assert doc.language == "en"
      assert doc.identifier == "urn:uuid:minimal-epub-3"
      assert doc.creators == ["Jane Doe"]
      assert doc.rights == "CC0 1.0"
      assert doc.version == "3.0"
    end

    test "spine has the one chapter and its data is non-empty XHTML", %{bytes: bytes} do
      assert {:ok, %Document{spine: [%Chapter{} = ch]}} = LangelicEpub.parse(bytes)
      assert ch.id == "chapter1"
      assert ch.media_type == "application/xhtml+xml"
      assert byte_size(ch.data) > 0
      assert ch.data =~ "Hello, world."
    end

    test "assets and cover are empty for a text-only minimal fixture", %{bytes: bytes} do
      assert {:ok, %Document{} = doc} = LangelicEpub.parse(bytes)
      assert doc.assets == []
      assert doc.cover_asset_id == nil
    end
  end

  describe "parse/1 with multiple creators" do
    test "preserves each dc:creator entry as a separate list element" do
      bytes =
        EpubFixtureBuilder.minimal_epub3(
          title: "Multi Author",
          creators: ["Alice", "Bob", "Carol"]
        )

      assert {:ok, %Document{creators: creators}} = LangelicEpub.parse(bytes)
      assert creators == ["Alice", "Bob", "Carol"]
    end
  end

  describe "parse/1 error paths" do
    test "empty bytes return an invalid_zip error" do
      assert {:error, %Error{kind: :invalid_zip}} = LangelicEpub.parse(<<>>)
    end

    test "zip-looking garbage returns an error without crashing" do
      garbage = "PK" <> :crypto.strong_rand_bytes(256)
      assert {:error, %Error{}} = LangelicEpub.parse(garbage)
    end

    test "random bytes that are not a zip return :invalid_zip" do
      assert {:error, %Error{kind: :invalid_zip}} =
               LangelicEpub.parse("not an epub at all")
    end
  end

  describe "parse/1 on the 8 spike sample EPUBs" do
    @sample_paths LangelicEpub.Fixtures.sample_paths()

    if @sample_paths == [] do
      @tag :skip
      test "spike samples missing — skipping" do
        flunk("test/support/fixtures/samples is empty; copy spike samples to run this suite")
      end
    else
      for path <- @sample_paths do
        name = Path.basename(path)

        test "parses #{name}" do
          bytes = File.read!(unquote(path))
          assert {:ok, %Document{} = doc} = LangelicEpub.parse(bytes)
          assert is_binary(doc.title)
          assert doc.title != ""
          assert is_binary(doc.identifier)
          # language must be recovered from the OPF re-parse even though
          # iepub drops it (spike finding #1).
          assert is_binary(doc.language),
                 "expected language to be populated (OPF re-parse check)"
          assert is_list(doc.spine)
          assert doc.spine != []

          for ch <- doc.spine do
            assert %Chapter{} = ch
            assert is_binary(ch.id)
            assert ch.id != ""
            assert is_binary(ch.data)
            assert byte_size(ch.data) > 0
          end

          for a <- doc.assets do
            assert %Asset{} = a
            assert byte_size(a.data) > 0
          end

          assert_nav_items(doc.toc)
        end
      end
    end
  end

  defp assert_nav_items(items) when is_list(items) do
    for item <- items do
      assert %NavItem{} = item
      assert is_binary(item.title)
      assert is_binary(item.href)
      assert_nav_items(item.children)
    end
  end
end
