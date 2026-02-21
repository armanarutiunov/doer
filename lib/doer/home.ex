defmodule Doer.Home do
  use TermUI.Elm

  alias Doer.Store
  alias Doer.Home.Helpers

  @pad_y_top 1
  @bottom_reserved 5
  @scroll_margin 5
  @prefix_w 4

  def pad_y_top, do: @pad_y_top
  def bottom_reserved, do: @bottom_reserved
  def scroll_margin, do: @scroll_margin
  def prefix_w, do: @prefix_w

  # --- Init ---

  def init(_opts) do
    todos = Store.load()
    {rows, cols} = TermUI.Platform.terminal_size()
    schedule_size_poll()

    %{
      mode: :normal,
      todos: todos,
      cursor: 0,
      visual_anchor: 0,
      editing_text: "",
      editing_id: nil,
      editing_original: "",
      search_text: "",
      search_matches: [],
      show_help: false,
      scroll_offset: 0,
      terminal_width: cols,
      terminal_height: rows
    }
  end

  def handle_info(:poll_size, state) do
    schedule_size_poll()
    {rows, cols} = TermUI.Platform.terminal_size()

    if cols != state.terminal_width or rows != state.terminal_height do
      {%{state | terminal_width: cols, terminal_height: rows} |> Helpers.adjust_scroll(), []}
    else
      state
    end
  end

  defp schedule_size_poll, do: Process.send_after(self(), :poll_size, 200)

  defdelegate event_to_msg(event, state), to: Doer.Home.EventMapping
  defdelegate update(msg, state), to: Doer.Home.Update
  defdelegate view(state), to: Doer.Home.View
end
