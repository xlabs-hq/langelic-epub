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
      assert {:error, %Error{kind: :missing_required_field, message: msg}} = LangelicEpub.build(doc)
      assert msg =~ "title"
    end

    test "rejects missing identifier" do
      doc = base_doc(identifier: "")
      assert {:error, %Error{kind: :missing_required_field, message: msg}} = LangelicEpub.build(doc)
      assert msg =~ "identifier"
    end

    test "rejects missing language (nil)" do
      doc = base_doc(language: nil)
      assert {:error, %Error{kind: :missing_required_field, message: msg}} = LangelicEpub.build(doc)
      assert msg =~ "language"
    end

    test "rejects empty-string language" do
      doc = base_doc(language: "")
      assert {:error, %Error{kind: :missing_required_field, message: msg}} = LangelicEpub.build(doc)
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
  end
end
