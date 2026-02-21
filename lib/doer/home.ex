defmodule Doer.Home do
  use TermUI.Elm

  alias TermUI.Event
  alias TermUI.Renderer.Style
  alias Doer.{Todo, Store}

  @pad_y_top 2
  @pad_y_bottom 1
  @prefix_w 4  # indicator(2) + checkbox(2)

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
      {%{state | terminal_width: cols, terminal_height: rows} |> adjust_scroll(), []}
    else
      state
    end
  end

  defp schedule_size_poll, do: Process.send_after(self(), :poll_size, 200)

  # --- Event to Msg ---

  # Resize
  def event_to_msg(%Event.Resize{width: w, height: h}, _state),
    do: {:msg, {:resize, w, h}}

  # Normal mode
  # Normal mode — ctrl combos first
  def event_to_msg(%Event.Key{key: "d", modifiers: [:ctrl]}, %{mode: :normal, show_help: false}),
    do: {:msg, :half_page_down}

  def event_to_msg(%Event.Key{key: "u", modifiers: [:ctrl]}, %{mode: :normal, show_help: false}),
    do: {:msg, :half_page_up}

  def event_to_msg(%Event.Key{key: "q", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :quit}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :normal, show_help: false})
      when key in ["j", :down],
      do: {:msg, :cursor_down}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :normal, show_help: false})
      when key in ["k", :up],
      do: {:msg, :cursor_up}

  def event_to_msg(%Event.Key{key: "a", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :add_todo}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :normal, show_help: false})
      when key in ["e", "i"],
      do: {:msg, :edit_todo}

  def event_to_msg(%Event.Key{key: "d", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :delete_todo}

  def event_to_msg(%Event.Key{key: " ", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :toggle_todo}

  def event_to_msg(%Event.Key{key: "v", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :enter_visual}

  def event_to_msg(%Event.Key{key: "?", modifiers: []}, %{mode: :normal}),
    do: {:msg, :toggle_help}

  def event_to_msg(%Event.Key{key: :escape}, %{show_help: true}),
    do: {:msg, :toggle_help}

  def event_to_msg(%Event.Key{key: "/", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :enter_search}

  def event_to_msg(%Event.Key{key: "G", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :cursor_end}

  def event_to_msg(%Event.Key{key: "g", modifiers: []}, %{mode: :normal, show_help: false}),
    do: {:msg, :cursor_start}

  # Insert mode
  def event_to_msg(%Event.Key{key: :enter}, %{mode: :insert}),
    do: {:msg, :confirm_edit}

  def event_to_msg(%Event.Key{key: :escape}, %{mode: :insert}),
    do: {:msg, :cancel_edit}

  def event_to_msg(%Event.Key{key: :backspace}, %{mode: :insert}),
    do: {:msg, :backspace}

  def event_to_msg(%Event.Key{key: key}, %{mode: :insert}) when is_binary(key) and byte_size(key) == 1,
    do: {:msg, {:type_char, key}}

  # Visual mode — ctrl combos first
  def event_to_msg(%Event.Key{key: key, modifiers: [:ctrl]}, %{mode: :visual})
      when key in ["j", :down],
      do: {:msg, :move_selected_down}

  def event_to_msg(%Event.Key{key: key, modifiers: [:ctrl]}, %{mode: :visual})
      when key in ["k", :up],
      do: {:msg, :move_selected_up}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :visual})
      when key in ["j", :down],
      do: {:msg, :visual_down}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :visual})
      when key in ["k", :up],
      do: {:msg, :visual_up}

  def event_to_msg(%Event.Key{key: "d", modifiers: []}, %{mode: :visual}),
    do: {:msg, :delete_selected}

  def event_to_msg(%Event.Key{key: " ", modifiers: []}, %{mode: :visual}),
    do: {:msg, :toggle_selected}

  def event_to_msg(%Event.Key{key: :escape}, %{mode: :visual}),
    do: {:msg, :exit_visual}

  # Search mode
  def event_to_msg(%Event.Key{key: :enter}, %{mode: :search}),
    do: {:msg, :confirm_search}

  def event_to_msg(%Event.Key{key: :escape}, %{mode: :search}),
    do: {:msg, :cancel_search}

  def event_to_msg(%Event.Key{key: :backspace}, %{mode: :search}),
    do: {:msg, :search_backspace}

  def event_to_msg(%Event.Key{key: key}, %{mode: :search}) when is_binary(key) and byte_size(key) == 1,
    do: {:msg, {:search_type, key}}

  # Search nav mode
  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :search_nav})
      when key in ["j", :down],
      do: {:msg, :cursor_down}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{mode: :search_nav})
      when key in ["k", :up],
      do: {:msg, :cursor_up}

  def event_to_msg(%Event.Key{key: "/", modifiers: []}, %{mode: :search_nav}),
    do: {:msg, :enter_search}

  def event_to_msg(%Event.Key{key: :escape}, %{mode: :search_nav}),
    do: {:msg, :cancel_search}

  def event_to_msg(_, _), do: :ignore

  # --- Update ---

  def update({:resize, w, h}, state),
    do: {%{state | terminal_width: w, terminal_height: h} |> adjust_scroll()}

  def update(:quit, state) do
    Store.save(state.todos)
    {state, [:quit]}
  end

  def update(:cursor_down, state) do
    max = max(length(combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + 1, max)} |> adjust_scroll()}
  end

  def update(:cursor_up, state) do
    {%{state | cursor: max(state.cursor - 1, 0)} |> adjust_scroll()}
  end

  def update(:cursor_end, state) do
    max = max(length(combined_list(state)) - 1, 0)
    {%{state | cursor: max} |> adjust_scroll()}
  end

  def update(:cursor_start, state) do
    {%{state | cursor: 0} |> adjust_scroll()}
  end

  def update(:half_page_down, state) do
    jump = div(state.terminal_height, 2)
    max = max(length(combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + jump, max)} |> adjust_scroll()}
  end

  def update(:half_page_up, state) do
    jump = div(state.terminal_height, 2)
    {%{state | cursor: max(state.cursor - jump, 0)} |> adjust_scroll()}
  end

  # Add todo
  def update(:add_todo, state) do
    new_todo = Todo.new("")
    active = Enum.filter(state.todos, &(!&1.done))
    insert_pos = min(state.cursor + 1, length(active))

    {before, after_list} = Enum.split(active, insert_pos)
    completed = Enum.filter(state.todos, & &1.done)
    new_todos = before ++ [new_todo] ++ after_list ++ completed

    {%{state |
      mode: :insert,
      todos: new_todos,
      cursor: insert_pos,
      editing_id: nil,
      editing_text: "",
      editing_original: ""
    } |> adjust_scroll()}
  end

  # Edit todo
  def update(:edit_todo, state) do
    combined = combined_list(state)
    case Enum.at(combined, state.cursor) do
      nil -> :noreply
      todo ->
        {%{state |
          mode: :insert,
          editing_id: todo.id,
          editing_text: todo.text,
          editing_original: todo.text
        }}
    end
  end

  # Confirm edit
  def update(:confirm_edit, state) do
    state = if state.editing_id == nil do
      # New todo via 'a'
      if String.trim(state.editing_text) == "" do
        active = Enum.filter(state.todos, &(!&1.done))
        todo = Enum.at(active, state.cursor)
        %{state | todos: Enum.reject(state.todos, &(&1.id == todo.id))}
      else
        active = Enum.filter(state.todos, &(!&1.done))
        todo = Enum.at(active, state.cursor)
        todos = Enum.map(state.todos, fn t ->
          if t.id == todo.id, do: %{t | text: state.editing_text}, else: t
        end)
        %{state | todos: todos}
      end
    else
      if String.trim(state.editing_text) == "" do
        %{state | todos: Enum.map(state.todos, fn t ->
          if t.id == state.editing_id, do: %{t | text: state.editing_original}, else: t
        end)}
      else
        %{state | todos: Enum.map(state.todos, fn t ->
          if t.id == state.editing_id, do: %{t | text: state.editing_text}, else: t
        end)}
      end
    end

    cursor = clamp_cursor(state.cursor, state.todos)
    state = %{state | mode: :normal, cursor: cursor, editing_id: nil, editing_text: "", editing_original: ""} |> adjust_scroll()
    Store.save(state.todos)
    {state}
  end

  # Cancel edit
  def update(:cancel_edit, state) do
    state = if state.editing_id == nil do
      active = Enum.filter(state.todos, &(!&1.done))
      todo = Enum.at(active, state.cursor)
      if todo, do: %{state | todos: Enum.reject(state.todos, &(&1.id == todo.id))}, else: state
    else
      %{state | todos: Enum.map(state.todos, fn t ->
        if t.id == state.editing_id, do: %{t | text: state.editing_original}, else: t
      end)}
    end

    cursor = clamp_cursor(state.cursor, state.todos)
    {%{state | mode: :normal, cursor: cursor, editing_id: nil, editing_text: "", editing_original: ""} |> adjust_scroll()}
  end

  def update(:backspace, state) do
    t = String.slice(state.editing_text, 0..-2//1)
    {%{state | editing_text: t}}
  end

  def update({:type_char, char}, state) do
    {%{state | editing_text: state.editing_text <> char}}
  end

  # Delete
  def update(:delete_todo, state) do
    combined = combined_list(state)
    case Enum.at(combined, state.cursor) do
      nil -> :noreply
      todo ->
        todos = Enum.reject(state.todos, &(&1.id == todo.id))
        cursor = clamp_cursor(state.cursor, todos)
        Store.save(todos)
        {%{state | todos: todos, cursor: cursor} |> adjust_scroll()}
    end
  end

  # Toggle
  def update(:toggle_todo, state) do
    combined = combined_list(state)
    case Enum.at(combined, state.cursor) do
      nil -> :noreply
      todo ->
        todos = Enum.map(state.todos, fn t ->
          if t.id == todo.id, do: Todo.toggle(t), else: t
        end)
        Store.save(todos)
        {%{state | todos: todos}}
    end
  end

  # Visual mode
  def update(:enter_visual, state),
    do: {%{state | mode: :visual, visual_anchor: state.cursor}}

  def update(:exit_visual, state),
    do: {%{state | mode: :normal}}

  def update(:visual_down, state) do
    max = max(length(combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + 1, max)} |> adjust_scroll()}
  end

  def update(:visual_up, state) do
    {%{state | cursor: max(state.cursor - 1, 0)} |> adjust_scroll()}
  end

  def update(:delete_selected, state) do
    selected_ids = selected_todo_ids(state)
    todos = Enum.reject(state.todos, &(&1.id in selected_ids))
    cursor = clamp_cursor(state.cursor, todos)
    Store.save(todos)
    {%{state | mode: :normal, todos: todos, cursor: cursor} |> adjust_scroll()}
  end

  def update(:toggle_selected, state) do
    selected_ids = selected_todo_ids(state)
    todos = Enum.map(state.todos, fn t ->
      if t.id in selected_ids, do: Todo.toggle(t), else: t
    end)
    Store.save(todos)
    {%{state | mode: :normal, todos: todos} |> adjust_scroll()}
  end

  def update(:move_selected_down, state) do
    active = Enum.filter(state.todos, &(!&1.done))
    {sel_min, sel_max} = selection_range(state)

    if sel_max < length(active) - 1 do
      active_list = Enum.with_index(active)
      {selected, rest} = Enum.split_with(active_list, fn {_, i} -> i >= sel_min and i <= sel_max end)
      {before_swap, [swap_item | after_swap]} = Enum.split(rest, sel_min)

      new_active =
        Enum.map(before_swap, &elem(&1, 0)) ++
        [elem(swap_item, 0)] ++
        Enum.map(selected, &elem(&1, 0)) ++
        Enum.map(after_swap, &elem(&1, 0))

      completed = Enum.filter(state.todos, & &1.done)
      {%{state | todos: new_active ++ completed, cursor: state.cursor + 1, visual_anchor: state.visual_anchor + 1} |> adjust_scroll()}
    else
      :noreply
    end
  end

  def update(:move_selected_up, state) do
    active = Enum.filter(state.todos, &(!&1.done))
    {sel_min, _sel_max} = selection_range(state)

    if sel_min > 0 do
      active_list = Enum.with_index(active)
      {selected, rest} = Enum.split_with(active_list, fn {_, i} -> i >= sel_min and i <= elem(selection_range(state), 1) end)
      {before, after_list} = Enum.split(rest, sel_min - 1)

      new_active =
        Enum.map(before, &elem(&1, 0)) ++
        Enum.map(selected, &elem(&1, 0)) ++
        Enum.map(after_list, &elem(&1, 0))

      completed = Enum.filter(state.todos, & &1.done)
      {%{state | todos: new_active ++ completed, cursor: state.cursor - 1, visual_anchor: state.visual_anchor - 1} |> adjust_scroll()}
    else
      :noreply
    end
  end

  # Search
  def update(:enter_search, state),
    do: {%{state | mode: :search, search_text: state.search_text}}

  def update(:confirm_search, state) do
    matches = filter_todos(state.todos, state.search_text)
    {%{state | mode: :search_nav, search_matches: Enum.map(matches, & &1.id), cursor: 0} |> adjust_scroll()}
  end

  def update(:cancel_search, state),
    do: {%{state | mode: :normal, search_text: "", search_matches: []}}

  def update(:search_backspace, state) do
    t = String.slice(state.search_text, 0..-2//1)
    {%{state | search_text: t}}
  end

  def update({:search_type, char}, state),
    do: {%{state | search_text: state.search_text <> char}}

  # Help
  def update(:toggle_help, state),
    do: {%{state | show_help: !state.show_help}}

  # --- View ---

  def view(state) do
    content_w = content_width(state.terminal_width)
    left_pad = div(state.terminal_width - content_w, 2)
    pad_str = String.duplicate(" ", left_pad)

    # Top padding
    top_pad = blank_rows(@pad_y_top)

    # Todo list rows
    list_rows = render_list(state, content_w, pad_str)
    list_content = stack(:vertical, list_rows)

    # Bottom section: search bar + mode bar
    bottom_rows = render_bottom(state, content_w, pad_str)
    bottom_row_count = if state.mode in [:search, :search_nav], do: 2, else: 1

    # Viewport for scrollable list
    vh = max(state.terminal_height - @pad_y_top - bottom_row_count - @pad_y_bottom, 1)

    viewport = %{
      type: :viewport,
      content: list_content,
      scroll_x: 0,
      scroll_y: state.scroll_offset,
      width: state.terminal_width,
      height: vh
    }

    bottom_pad = blank_rows(@pad_y_bottom)

    all = top_pad ++ [viewport] ++ bottom_rows ++ bottom_pad
    base = stack(:vertical, all)

    if state.show_help do
      help = render_help(left_pad)
      stack(:vertical, [base, help])
    else
      base
    end
  end

  # --- Private: Layout ---

  defp content_width(tw) do
    max(trunc(tw * 0.6), 20)
  end

  defp blank_rows(0), do: []
  defp blank_rows(n), do: Enum.map(1..n, fn _ -> text("", nil) end)

  # --- Private: View helpers ---

  defp render_list(state, content_w, pad_str) do
    {disp_active, disp_completed} = display_todos(state)

    active_header = if length(disp_active) > 0 do
      [render_section_header("Todos", "Created", content_w, pad_str)]
    else
      []
    end

    active_rows = disp_active
      |> Enum.with_index()
      |> Enum.flat_map(fn {todo, idx} -> render_todo_row(todo, idx, state, false, content_w, pad_str) end)

    section_spacing = if length(disp_completed) > 0, do: blank_rows(2), else: []

    completed_header = if length(disp_completed) > 0 do
      [render_section_header("Completed", "Created  Completed", content_w, pad_str)]
    else
      []
    end

    spacing_above_completed = if length(disp_completed) > 0, do: blank_rows(1), else: []

    completed_rows = disp_completed
      |> Enum.with_index(length(disp_active))
      |> Enum.flat_map(fn {todo, idx} -> render_todo_row(todo, idx, state, true, content_w, pad_str) end)

    active_header ++ active_rows ++ section_spacing ++ completed_header ++ spacing_above_completed ++ completed_rows
  end

  defp render_section_header(title, date_label, content_w, pad_str) do
    dim = Style.new(fg: :bright_black)
    prefix = String.duplicate(" ", @prefix_w)
    date_w = String.length(date_label)
    title_w = content_w - @prefix_w - date_w - 2
    padded_title = String.pad_trailing(title, max(title_w, 0))

    stack(:horizontal, [
      text(pad_str, nil),
      text(prefix, nil),
      text(padded_title, dim),
      text("  " <> date_label, dim)
    ])
  end

  defp render_todo_row(todo, idx, state, is_completed, content_w, pad_str) do
    is_cursor = idx == state.cursor
    is_selected = state.mode == :visual and idx in visual_range(state)
    is_editing = state.mode == :insert and
      ((state.editing_id == nil and idx == state.cursor) or
       (state.editing_id == todo.id))

    # Determine age strings
    age_str = Todo.age_label(todo)
    completed_age_str = if is_completed and todo.completed_at, do: Todo.completed_label(todo), else: nil

    # Right column: "  0d" or "  0d  0d"
    right_col = "  " <> String.pad_leading(age_str, 4)
    right_col = if completed_age_str, do: right_col <> "  " <> String.pad_leading(completed_age_str, 4), else: right_col
    right_w = String.length(right_col)

    # Available width for text (after indicator + checkbox, before age)
    text_area_w = max(content_w - @prefix_w - right_w, 10)

    # Build display text
    display_text = if is_editing, do: state.editing_text <> "█", else: todo.text

    # Wrap text into lines
    lines = wrap_text(display_text, text_area_w)

    # Indicator + checkbox prefix
    indicator = if is_selected, do: "▎ ", else: "  "
    checkbox = if is_completed, do: "◉ ", else: "◯ "
    prefix = indicator <> checkbox
    continuation_prefix = String.duplicate(" ", @prefix_w)

    # Style
    text_style = cond do
      is_editing -> Style.new(fg: :green)
      is_completed -> Style.new(fg: :bright_black, attrs: MapSet.new([:strikethrough]))
      is_cursor -> Style.new(fg: :white, attrs: MapSet.new([:bold]))
      true -> nil
    end

    right_style = Style.new(fg: :bright_black)

    cursor_bg = if is_cursor and not is_editing, do: Style.new(bg: {:rgb, 55, 51, 84}), else: nil

    # Build rows — first line has prefix + text + right-aligned age
    lines
    |> Enum.with_index()
    |> Enum.map(fn {line, line_idx} ->
      padding = String.duplicate(" ", max(text_area_w - String.length(line), 0))

      {pfx, age_text} = if line_idx == 0 do
        {prefix, right_col}
      else
        {continuation_prefix, String.duplicate(" ", right_w)}
      end

      prefix_style = if(line_idx == 0 and is_selected, do: Style.new(fg: :magenta), else: nil)

      content = stack(:horizontal, [
        text(pfx, prefix_style),
        text(line, text_style),
        text(padding, nil),
        text(age_text, right_style)
      ])

      content = if cursor_bg, do: styled(content, cursor_bg), else: content

      stack(:horizontal, [
        text(pad_str, nil),
        content
      ])
    end)
  end

  defp render_bottom(state, _content_w, pad_str) do
    search_bar = if state.mode in [:search, :search_nav] do
      search_text = "/" <> state.search_text <> "█"
      [text(pad_str <> search_text, Style.new(fg: :white))]
    else
      []
    end

    {label, bg_color} = case state.mode do
      :normal -> {"NORMAL", :blue}
      :visual -> {"VISUAL", :magenta}
      :insert -> {"INSERT", :green}
      :search -> {"SEARCH", :yellow}
      :search_nav -> {"SEARCH", :yellow}
    end

    hint = if state.mode == :normal and not state.show_help,
      do: text("  ? for help", Style.new(fg: :bright_black)),
      else: text("", nil)

    mode_bar = [stack(:horizontal, [
      text(pad_str, nil),
      text(" -- ", Style.new(fg: :bright_black)),
      text(" #{label} ", Style.new(fg: :black, bg: bg_color)),
      text(" -- ", Style.new(fg: :bright_black)),
      hint
    ])]

    search_bar ++ mode_bar
  end

  defp render_help(left_pad) do
    lines = [
      "Keybindings",
      "",
      "j/k/↑/↓   navigate",
      "a          add todo",
      "e/i        edit todo",
      "d          delete todo",
      "space      toggle done",
      "v          visual mode",
      "/          search",
      "G          go to end",
      "g          go to start",
      "ctrl+d     half page down",
      "ctrl+u     half page up",
      "?          toggle help",
      "q          quit"
    ]

    inner_w = 30
    border_style = Style.new(fg: :bright_black, bg: :black)
    text_style = Style.new(fg: :white, bg: :black)

    top_border = text("┌" <> String.duplicate("─", inner_w) <> "┐", border_style)
    bottom_border = text("└" <> String.duplicate("─", inner_w) <> "┘", border_style)

    content_rows = Enum.map(lines, fn line ->
      padded = String.pad_trailing(" " <> line, inner_w)
      stack(:horizontal, [
        text("│", border_style),
        text(padded, text_style),
        text("│", border_style)
      ])
    end)

    help_content = stack(:vertical, [top_border] ++ content_rows ++ [bottom_border])

    %{
      type: :overlay,
      content: help_content,
      x: left_pad,
      y: @pad_y_top,
      z: 100,
      width: inner_w + 2,
      height: length(lines) + 2,
      bg: Style.new(bg: :black)
    }
  end

  # --- Private: Text wrapping ---

  defp wrap_text("", _width), do: [""]

  defp wrap_text(text_str, width) when width > 0 do
    words = String.split(text_str, " ")
    do_wrap(words, width, [], "")
  end

  defp do_wrap([], _width, lines, current) do
    Enum.reverse([current | lines])
  end

  defp do_wrap([word | rest], width, lines, "") do
    if String.length(word) > width do
      # Word itself is longer than width — hard break it
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

  # --- Private: Display helpers ---

  defp display_todos(state) do
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

  # --- Private: State helpers ---

  defp viewport_height(state) do
    # bottom bar = 1 (mode) + 1 if searching
    bottom_rows = if state.mode in [:search, :search_nav], do: 2, else: 1
    max(state.terminal_height - @pad_y_top - bottom_rows - @pad_y_bottom, 1)
  end

  defp adjust_scroll(state) do
    vh = viewport_height(state)
    offset = state.scroll_offset

    offset = if state.cursor < offset, do: state.cursor, else: offset
    offset = if state.cursor >= offset + vh, do: state.cursor - vh + 1, else: offset
    offset = max(offset, 0)

    %{state | scroll_offset: offset}
  end

  defp combined_list(state) do
    active = Enum.filter(state.todos, &(!&1.done))
    completed = Enum.filter(state.todos, & &1.done)
    active ++ completed
  end

  defp clamp_cursor(cursor, todos) do
    combined = Enum.filter(todos, &(!&1.done)) ++ Enum.filter(todos, & &1.done)
    max_idx = max(length(combined) - 1, 0)
    min(cursor, max_idx)
  end

  defp selection_range(%{cursor: cursor, visual_anchor: anchor}) do
    {min(cursor, anchor), max(cursor, anchor)}
  end

  defp visual_range(state) do
    {sel_min, sel_max} = selection_range(state)
    Enum.to_list(sel_min..sel_max)
  end

  defp selected_todo_ids(state) do
    combined = combined_list(state)
    visual_range(state)
    |> Enum.map(&Enum.at(combined, &1))
    |> Enum.reject(&is_nil/1)
    |> Enum.map(& &1.id)
  end

  defp filter_todos(todos, query) do
    q = String.downcase(query)
    Enum.filter(todos, fn t ->
      String.contains?(String.downcase(t.text), q)
    end)
  end
end
