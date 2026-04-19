defmodule LangelicEpub.Chapter do
  @moduledoc """
  A chapter in the spine. `data` is the raw XHTML bytes — the library does not
  parse or rewrite chapter HTML, that is the caller's responsibility.
  """

  @type t :: %__MODULE__{
          id: String.t(),
          file_name: String.t(),
          title: String.t() | nil,
          media_type: String.t(),
          data: binary()
        }

  defstruct [:id, :file_name, :title, :media_type, :data]
end
