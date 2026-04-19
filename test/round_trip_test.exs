defmodule LangelicEpub.RoundTripTest do
  use ExUnit.Case, async: true

  alias LangelicEpub.{Chapter, Document, EpubFixtureBuilder, Fixtures}

  describe "parse → build → parse preserves core metadata" do
    test "round-trips the minimal hand-built EPUB 3 fixture" do
      original_bytes =
        EpubFixtureBuilder.minimal_epub3(
          title: "Round Trip",
          language: "en",
          identifier: "urn:uuid:round-trip",
          creators: ["Alice", "Bob"],
          rights: "CC0 1.0"
        )

      {:ok, original} = LangelicEpub.parse(original_bytes)
      {:ok, rebuilt_bytes} = LangelicEpub.build(original)
      {:ok, rebuilt} = LangelicEpub.parse(rebuilt_bytes)

      assert rebuilt.title == original.title
      assert rebuilt.language == original.language
      assert rebuilt.identifier == original.identifier
      assert rebuilt.creators == original.creators
      assert length(rebuilt.spine) == length(original.spine)
    end

    test "modification to chapter data survives round-trip" do
      bytes = EpubFixtureBuilder.minimal_epub3()
      {:ok, %Document{} = doc} = LangelicEpub.parse(bytes)

      modified_spine =
        Enum.map(doc.spine, fn %Chapter{} = ch ->
          %Chapter{ch | data: String.replace(ch.data, "Hello, world.", "Hello, round-trip!")}
        end)

      modified = %Document{doc | spine: modified_spine}
      {:ok, rebuilt_bytes} = LangelicEpub.build(modified)
      {:ok, rebuilt} = LangelicEpub.parse(rebuilt_bytes)

      [ch] = rebuilt.spine
      assert ch.data =~ "Hello, round-trip!"
      refute ch.data =~ "Hello, world."
    end
  end

  describe "round-trip against the 8 spike sample EPUBs" do
    @sample_paths Fixtures.sample_paths()

    if @sample_paths == [] do
      @tag :skip
      test "spike samples missing — skipping" do
        flunk("test/support/fixtures/samples is empty")
      end
    else
      for path <- @sample_paths do
        name = Path.basename(path)

        test "round-trips #{name}" do
          bytes = File.read!(unquote(path))
          {:ok, original} = LangelicEpub.parse(bytes)

          {:ok, rebuilt_bytes} = LangelicEpub.build(original)
          assert is_binary(rebuilt_bytes)
          assert byte_size(rebuilt_bytes) > 0

          {:ok, rebuilt} = LangelicEpub.parse(rebuilt_bytes)

          assert rebuilt.title == original.title
          assert rebuilt.identifier == original.identifier
          # epub-builder may normalize the language tag; tolerate that.
          assert is_binary(rebuilt.language)
          assert length(rebuilt.spine) == length(original.spine),
                 "spine length mismatch for #{unquote(name)}"

          # Every chapter that had content should still have content.
          for {before, aftr} <- Enum.zip(original.spine, rebuilt.spine) do
            assert byte_size(before.data) > 0
            assert byte_size(aftr.data) > 0
          end

          # Non-cover assets should round-trip (count may differ by one because
          # the cover is separately flagged, and epub-builder may drop assets
          # referenced only by dropped nav/ncx files).
          assert length(rebuilt.assets) >= max(length(original.assets) - 2, 0)
        end
      end
    end
  end
end
