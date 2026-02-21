defmodule Doer.Home.View do
  import TermUI.Component.Helpers

  alias TermUI.Renderer.Style
  alias Doer.{Todo, Home}
  alias Doer.Home.Helpers

  def view(state) do
    content_w = content_width(state.terminal_width)
    left_pad = div(state.terminal_width - content_w, 2)
    pad_str = String.duplicate(" ", left_pad)

    # Top padding
    top_pad = blank_rows(Home.pad_y_top())

    # Todo list rows — slice visible range directly (no viewport)
    list_rows = render_list(state, content_w, pad_str)
    vh = max(state.terminal_height - Home.pad_y_top() - Home.bottom_reserved(), 1)

    # Clamp scroll so we don't over-scroll past content
    max_offset = max(length(list_rows) - vh, 0)
    scroll = min(state.scroll_offset, max_offset)

    visible_rows =
      list_rows
      |> Enum.drop(scroll)
      |> Enum.take(vh)

    # Pad to push bottom section to the bottom when content is short
    pad_count = vh - length(visible_rows)
    visible_pad = if pad_count > 0, do: blank_rows(pad_count), else: []

    # Bottom section: spacer + search/empty + blank + mode bar
    bottom_rows = render_bottom(state, content_w, pad_str)

    all = top_pad ++ visible_rows ++ visible_pad ++ bottom_rows
    base = stack(:vertical, all)

    if state.show_help do
      help = render_help(state.terminal_width, state.terminal_height)
      stack(:vertical, [base, help])
    else
      base
    end
  end

  # Offset RGB blue channel by 1 per row to prevent terminal renderer
  # from merging adjacent cells with identical styles during scroll
  defp unique_rgb({r, g, b}, idx) when b >= 255, do: {r, g, b - rem(idx, 2)}
  defp unique_rgb({r, g, b}, idx), do: {r, g, b + rem(idx, 2)}

  def content_width(tw) do
    max(trunc(tw * 0.6), 20)
  end

  def blank_rows(0), do: []
  def blank_rows(n), do: Enum.map(1..n, fn _ -> text("", nil) end)

  def render_list(state, content_w, pad_str) do
    {disp_active, disp_completed} = Helpers.display_todos(state)

    active_header = [render_section_header("Todos", "Created", content_w, pad_str)]
    active_header_spacing = blank_rows(1)

    active_rows =
      if length(disp_active) == 0 do
        empty_style = Style.new(fg: :bright_black)

        [
          stack(:horizontal, [
            text(pad_str, nil),
            text(String.duplicate(" ", Home.prefix_w()), nil),
            text("press 'a' to add a new todo", empty_style)
          ])
        ]
      else
        disp_active
        |> Enum.with_index()
        |> Enum.flat_map(fn {todo, idx} ->
          render_todo_row(todo, idx, state, false, content_w, pad_str)
        end)
      end

    section_spacing = if length(disp_completed) > 0, do: blank_rows(2), else: []

    completed_header =
      if length(disp_completed) > 0 do
        [render_section_header("Completed", "Created  Completed", content_w, pad_str, 1)]
      else
        []
      end

    spacing_above_completed = if length(disp_completed) > 0, do: blank_rows(1), else: []

    completed_rows =
      disp_completed
      |> Enum.with_index(length(disp_active))
      |> Enum.flat_map(fn {todo, idx} ->
        render_todo_row(todo, idx, state, true, content_w, pad_str)
      end)

    active_header ++
      active_header_spacing ++
      active_rows ++
      section_spacing ++ completed_header ++ spacing_above_completed ++ completed_rows
  end

  def render_section_header(title, date_label, content_w, pad_str, idx \\ 0) do
    dim = Style.new(fg: unique_rgb({100, 100, 100}, idx))
    prefix = String.duplicate(" ", Home.prefix_w())
    date_w = String.length(date_label)
    title_w = content_w - Home.prefix_w() - date_w - 2
    padded_title = String.pad_trailing(title, max(title_w, 0))

    stack(:horizontal, [
      text(pad_str, nil),
      text(prefix, nil),
      text(padded_title, dim),
      text("  " <> date_label, dim)
    ])
  end

  def render_todo_row(todo, idx, state, is_completed, content_w, pad_str) do
    is_cursor = idx == state.cursor
    is_selected = state.mode == :visual and idx in Helpers.visual_range(state)

    is_editing =
      state.mode == :insert and
        ((state.editing_id == nil and idx == state.cursor) or
           state.editing_id == todo.id)

    # Determine age strings
    age_str = Todo.age_label(todo)

    completed_age_str =
      if is_completed and todo.completed_at, do: Todo.completed_label(todo), else: nil

    # Right column: aligned with "Created" (7) or "Created  Completed" (7+2+9)
    right_col = "  " <> String.pad_leading(age_str, 7)

    right_col =
      if completed_age_str,
        do: right_col <> "  " <> String.pad_leading(completed_age_str, 9),
        else: right_col

    right_w = String.length(right_col)

    # Available width for text (after indicator + checkbox, before age)
    text_area_w = max(content_w - Home.prefix_w() - right_w, 10)

    # Build display text
    display_text = if is_editing, do: state.editing_text <> "█", else: todo.text

    # Wrap text into lines
    lines = Helpers.wrap_text(display_text, text_area_w)

    # Indicator + checkbox prefix
    indicator = if is_selected, do: "▎ ", else: "  "
    checkbox = if is_completed, do: "◉ ", else: "◯ "
    prefix = indicator <> checkbox
    continuation_prefix = String.duplicate(" ", Home.prefix_w())

    # Style
    text_style =
      cond do
        is_editing -> Style.new(fg: :green)
        is_completed -> Style.new(fg: unique_rgb({80, 80, 80}, idx), attrs: [:strikethrough])
        is_cursor -> Style.new(fg: :white, attrs: [:bold])
        true -> nil
      end

    right_style = Style.new(fg: unique_rgb({140, 140, 140}, idx))

    cursor_bg = if is_cursor and not is_editing, do: Style.new(bg: {55, 51, 84}), else: nil

    # Build rows — first line has prefix + text + right-aligned age
    lines
    |> Enum.with_index()
    |> Enum.map(fn {line, line_idx} ->
      padding = String.duplicate(" ", max(text_area_w - String.length(line), 0))

      {pfx, age_text} =
        if line_idx == 0 do
          {prefix, right_col}
        else
          {continuation_prefix, String.duplicate(" ", right_w)}
        end

      prefix_style = if(line_idx == 0 and is_selected, do: Style.new(fg: unique_rgb({255, 122, 178}, idx)), else: nil)

      content =
        stack(:horizontal, [
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

  def render_bottom(state, content_w, pad_str) do
    # Always 3 lines: spacer + search_or_empty + mode_bar
    spacer = [text("", nil)]

    search_line =
      if state.mode in [:search, :search_nav] do
        search_text = "/" <> state.search_text <> "█"
        [text(pad_str <> search_text, Style.new(fg: :white))]
      else
        [text("", nil)]
      end

    {label, bg_color} =
      case state.mode do
        :normal -> {"NORMAL", :blue}
        :visual -> {"VISUAL", :magenta}
        :insert -> {"INSERT", :green}
        :search -> {"SEARCH", :yellow}
        :search_nav -> {"SEARCH", :yellow}
      end

    mode_text = " #{label} "
    mode_w = String.length(mode_text)

    # Completed count
    total = length(state.todos)
    done = Enum.count(state.todos, & &1.done)
    count_text = "#{done}/#{total} completed"
    count_w = String.length(count_text)

    # Help hint
    hint_text = if state.mode == :normal and not state.show_help, do: "? for help", else: ""
    hint_w = String.length(hint_text)

    # Padding between mode, count, and hint to fill content_w
    remaining = max(content_w - mode_w - count_w - hint_w, 0)
    left_gap = div(remaining, 2)
    right_gap = remaining - left_gap

    mode_bar = [
      stack(:horizontal, [
        text(pad_str, nil),
        text(mode_text, Style.new(fg: :black, bg: bg_color)),
        text(String.duplicate(" ", left_gap), nil),
        text(count_text, Style.new(fg: unique_rgb({140, 140, 140}, 0))),
        text(String.duplicate(" ", right_gap), nil),
        text(hint_text, Style.new(fg: unique_rgb({140, 140, 140}, 1)))
      ])
    ]

    spacer ++ search_line ++ [text("", nil)] ++ mode_bar ++ [text("", nil)]
  end

  def render_help(tw, th) do
    lines = [
      "",
      "Keybindings",
      "",
      "j/k/↑/↓   navigate",
      "a          add todo",
      "e/i        edit todo",
      "d          delete todo",
      "space      toggle done",
      "v          visual mode",
      "J/K        reorder (visual)",
      "/          search",
      "G          go to end",
      "g          go to start",
      "ctrl+d     half page down",
      "ctrl+u     half page up",
      "?          toggle help",
      "q          quit",
      ""
    ]

    pad_x = 4
    inner_w = 30
    box_w = inner_w + pad_x * 2
    box_h = length(lines)
    side = String.duplicate(" ", pad_x)

    content_rows =
      lines
      |> Enum.with_index()
      |> Enum.map(fn {line, i} ->
        s = Style.new(fg: :white, bg: {65, 65, 72 + rem(i, 2)})
        text(side <> String.pad_trailing(line, inner_w) <> side, s)
      end)

    help_content = stack(:vertical, content_rows)

    %{
      type: :overlay,
      content: help_content,
      x: max(div(tw - box_w, 2), 0),
      y: max(div(th - box_h, 2), 0),
      z: 100,
      width: box_w,
      height: box_h,
      bg: nil
    }
  end
end
