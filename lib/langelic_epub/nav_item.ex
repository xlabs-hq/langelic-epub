defmodule LangelicEpub.NavItem do
  @moduledoc """
  A node in the table of contents. May contain nested children.
  """

  @type t :: %__MODULE__{
          title: String.t(),
          href: String.t(),
          children: [t()]
        }

  defstruct [:title, :href, children: []]
end
