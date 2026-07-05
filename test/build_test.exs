defmodule LangelicEpub.BuildTest do
  use ExUnit.Case, async: true

  alias LangelicEpub.{Asset, Chapter, Document, Error}

  defp base_doc(opts \\ []) do
    %Document{
      title: Keyword.get(opts, :title, "A Title"),
      identifier: Keyword.get(opts, :identifier, "urn:uuid:build-test"),
      language: Keyword.get(opts, :language, "en"),
      creators: Keyword.get(opts, :creators, ["Tester"]),
      spine:
        Keyword.get(opts, :spine, [
          %Chapter{
            id: "ch1",
            file_name: "ch1.xhtml",
            media_type: "application/xhtml+xml",
            data: ~s|<?xml version="1.0"?><html><body><p>x</p></body></html>|
          }
        ]),
      assets: Keyword.get(opts, :assets, [])
    }
  end

  describe "build/1 validation" do
    test "rejects missing title" do
      doc = base_doc(title: "")

      assert {:error, %Error{kind: :missing_required_field, message: msg}} =
               LangelicEpub.build(doc)

      assert msg =~ "title"
    end

    test "rejects missing identifier" do
      doc = base_doc(identifier: "")

      assert {:error, %Error{kind: :missing_required_field, message: msg}} =
               LangelicEpub.build(doc)

      assert msg =~ "identifier"
    end

    test "rejects missing language (nil)" do
      doc = base_doc(language: nil)

      assert {:error, %Error{kind: :missing_required_field, message: msg}} =
               LangelicEpub.build(doc)

      assert msg =~ "language"
    end

    test "rejects empty-string language" do
      doc = base_doc(language: "")

      assert {:error, %Error{kind: :missing_required_field, message: msg}} =
               LangelicEpub.build(doc)

      assert msg =~ "language"
    end

    test "rejects duplicate ids across spine and assets" do
      doc =
        base_doc(
          spine: [
            %Chapter{
              id: "dup",
              file_name: "a.xhtml",
              media_type: "application/xhtml+xml",
              data: "<html/>"
            }
          ],
          assets: [
            %Asset{
              id: "dup",
              file_name: "style.css",
              media_type: "text/css",
              data: "body{}"
            }
          ]
        )

      assert {:error, %Error{kind: :duplicate_id, message: msg}} = LangelicEpub.build(doc)
      assert msg =~ "dup"
    end

    test "rejects duplicate ids within spine" do
      doc =
        base_doc(
          spine: [
            %Chapter{
              id: "x",
              file_name: "a.xhtml",
              media_type: "application/xhtml+xml",
              data: "<html/>"
            },
            %Chapter{
              id: "x",
              file_name: "b.xhtml",
              media_type: "application/xhtml+xml",
              data: "<html/>"
            }
          ]
        )

      assert {:error, %Error{kind: :duplicate_id}} = LangelicEpub.build(doc)
    end

    test "rejects non-UTF-8 chapter data" do
      doc =
        base_doc(
          spine: [
            %Chapter{
              id: "bad",
              file_name: "a.xhtml",
              media_type: "application/xhtml+xml",
              data: <<0xFF, 0xFE, 0xFD>>
            }
          ]
        )

      assert {:error, %Error{kind: :invalid_chapter, message: msg}} = LangelicEpub.build(doc)
      assert msg =~ "bad"
    end
  end

  describe "build/1 happy path" do
    test "produces non-empty bytes for a minimal valid document" do
      assert {:ok, bytes} = LangelicEpub.build(base_doc())
      assert is_binary(bytes)
      assert byte_size(bytes) > 0
      # EPUB files are zip archives; the first two bytes are "PK".
      assert <<"PK", _::binary>> = bytes
    end

    test "generated nav.xhtml carries no empty landmarks nav" do
      assert {:ok, bytes} = LangelicEpub.build(base_doc())

      {:ok, handle} = :zip.zip_open(bytes, [:memory])
      {:ok, {_, nav}} = :zip.zip_get(~c"OEBPS/nav.xhtml", handle)
      :zip.zip_close(handle)
      nav = to_string(nav)

      # epub-builder always emits an empty <nav epub:type="landmarks"> wrapper,
      # which fails epubcheck RSC-005; the writer's post-processing must strip
      # it. The toc nav (which has entries) must survive.
      refute nav =~ "landmarks"
      assert nav =~ ~s|epub:type = "toc"|
      assert nav =~ "<li>"
    end

    test "NCX playOrder is sequential and duplicate targets share one value" do
      # Reproduce the real-book shape that used to trip epubcheck RSC-005
      # ("different playOrder values ... refer to same target"): the same file
      # appears in the TOC both as a nested child and as a top-level entry.
      chapter = fn id, file ->
        %Chapter{
          id: id,
          file_name: file,
          media_type: "application/xhtml+xml",
          data:
            ~s|<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>#{id}</title></head><body><p>x</p></body></html>|
        }
      end

      doc = %Document{
        title: "PlayOrder",
        identifier: "urn:uuid:playorder-test",
        language: "en",
        creators: ["Tester"],
        spine: [chapter.("ch1", "ch1.xhtml"), chapter.("ch2", "ch2.xhtml")],
        toc: [
          %LangelicEpub.NavItem{
            title: "One",
            href: "ch1.xhtml",
            children: [
              %LangelicEpub.NavItem{title: "Nested Two", href: "ch2.xhtml", children: []}
            ]
          },
          %LangelicEpub.NavItem{title: "Two", href: "ch2.xhtml", children: []}
        ]
      }

      assert {:ok, bytes} = LangelicEpub.build(doc)

      {:ok, handle} = :zip.zip_open(bytes, [:memory])
      {:ok, {_, ncx}} = :zip.zip_get(~c"OEBPS/toc.ncx", handle)
      :zip.zip_close(handle)
      ncx = to_string(ncx)

      orders = Regex.scan(~r/playOrder="(\d+)"/, ncx, capture: :all_but_first)
      srcs = Regex.scan(~r/<content src="([^"]+)"/, ncx, capture: :all_but_first)
      pairs = Enum.zip(List.flatten(srcs), List.flatten(orders))

      # ch1 first (1), ch2 nested (2), ch2 top-level reuses 2 — never a fresh 3.
      assert pairs == [{"ch1.xhtml", "1"}, {"ch2.xhtml", "2"}, {"ch2.xhtml", "2"}]
    end
  end
end
