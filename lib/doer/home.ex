defmodule Doer.Home do
  use TermUI.Elm

  alias TermUI.Event
  alias TermUI.Renderer.Style

  def init(_opts), do: %{}

  def event_to_msg(%Event.Key{key: "q"}, _state), do: {:msg, :quit}
  def event_to_msg(_, _), do: :ignore

  def update(:quit, state), do: {state, [:quit]}

  def view(_state) do
    stack(:vertical, [
      text("doer", Style.new(fg: :cyan, attrs: [:bold])),
      text("", nil),
      text("Hello, World!", nil),
      text("", nil),
      text("Press Q to quit", Style.new(fg: :bright_black))
    ])
  end
end
