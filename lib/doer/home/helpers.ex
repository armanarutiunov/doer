defmodule Doer.Home.Helpers do
  alias Doer.Home

  def viewport_height(state) do
    max(state.terminal_height - Home.pad_y_top() - Home.bottom_reserved(), 1)
  end

  def adjust_scroll(state) do
    vh = viewport_height(state)
    row = cursor_visual_row(state)
    offset = state.scroll_offset
    margin = min(Home.scroll_margin(), div(vh, 2))

    offset = if row < offset + margin, do: max(row - margin, 0), else: offset
    offset = if row >= offset + vh - margin, do: row - vh + margin + 1, else: offset
    offset = max(offset, 0)

    %{state | scroll_offset: offset}
  end

  def cursor_visual_row(state) do
    {disp_active, disp_completed} = display_todos(state)
    active_count = length(disp_active)
    completed_count = length(disp_completed)

    active_header_rows = 2

    if state.cursor < active_count do
      active_header_rows + state.cursor
    else
      completed_idx = state.cursor - active_count
      base = active_header_rows + max(active_count, 1)
      separator = if completed_count > 0, do: 4, else: 0
      base + separator + completed_idx
    end
  end

  def combined_list(state) do
    active = Enum.filter(state.todos, &(!&1.done))
    completed = Enum.filter(state.todos, & &1.done)
    active ++ completed
  end

  def clamp_cursor(cursor, todos) do
    combined = Enum.filter(todos, &(!&1.done)) ++ Enum.filter(todos, & &1.done)
    max_idx = max(length(combined) - 1, 0)
    min(cursor, max_idx)
  end

  def selection_range(%{cursor: cursor, visual_anchor: anchor}) do
    {min(cursor, anchor), max(cursor, anchor)}
  end

  def visual_range(state) do
    {sel_min, sel_max} = selection_range(state)
    Enum.to_list(sel_min..sel_max)
  end

  def selected_todo_ids(state) do
    combined = combined_list(state)

    visual_range(state)
    |> Enum.map(&Enum.at(combined, &1))
    |> Enum.reject(&is_nil/1)
    |> Enum.map(& &1.id)
  end

  def filter_todos(todos, query) do
    q = String.downcase(query)

    Enum.filter(todos, fn t ->
      String.contains?(String.downcase(t.text), q)
    end)
  end

  def display_todos(state) do
    active = Enum.filter(state.todos, &(!&1.done))
    completed = Enum.filter(state.todos, & &1.done)

    case state.mode do
      mode when mode in [:search, :search_nav] and state.search_text != "" ->
        filtered = filter_todos(state.todos, state.search_text)
        {Enum.filter(filtered, &(!&1.done)), Enum.filter(filtered, & &1.done)}

      _ ->
        {active, completed}
    end
  end

  def wrap_text("", _width), do: [""]

  def wrap_text(text_str, width) when width > 0 do
    words = String.split(text_str, " ")
    do_wrap(words, width, [], "")
  end

  defp do_wrap([], _width, lines, current) do
    Enum.reverse([current | lines])
  end

  defp do_wrap([word | rest], width, lines, "") do
    if String.length(word) > width do
      chunks = hard_break(word, width)
      {full_chunks, [last]} = Enum.split(chunks, -1)
      do_wrap(rest, width, Enum.reverse(full_chunks) ++ lines, last)
    else
      do_wrap(rest, width, lines, word)
    end
  end

  defp do_wrap([word | rest], width, lines, current) do
    candidate = current <> " " <> word

    if String.length(candidate) <= width do
      do_wrap(rest, width, lines, candidate)
    else
      if String.length(word) > width do
        chunks = hard_break(word, width)
        {full_chunks, [last]} = Enum.split(chunks, -1)
        do_wrap(rest, width, Enum.reverse(full_chunks) ++ [current | lines], last)
      else
        do_wrap(rest, width, [current | lines], word)
      end
    end
  end

  defp hard_break(str, width) do
    str
    |> String.graphemes()
    |> Enum.chunk_every(width)
    |> Enum.map(&Enum.join/1)
  end

  def flat_ordered_projects(projects) do
    parents =
      projects
      |> Enum.filter(&is_nil(&1.parent_id))
      |> Enum.sort_by(& &1.index)

    Enum.flat_map(parents, fn parent ->
      children =
        projects
        |> Enum.filter(&(&1.parent_id == parent.id))
        |> Enum.sort_by(& &1.index)

      [parent | children]
    end)
  end
end
