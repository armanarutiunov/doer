defmodule Doer.Home.View do
  import TermUI.Component.Helpers

  alias TermUI.Renderer.Style
  alias Doer.{Todo, Home}
  alias Doer.Home.Helpers

  def view(state) do
    base =
      if state.sidebar_open do
        available_w = state.terminal_width - Home.sidebar_width()
        stack(:horizontal, [render_sidebar(state), render_main(state, available_w)])
      else
        render_main(state, state.terminal_width)
      end

    if state.show_help do
      help = render_help(state.terminal_width, state.terminal_height)
      stack(:vertical, [base, help])
    else
      base
    end
  end

  # --- Sidebar ---

  def render_sidebar(state) do
    sw = Home.sidebar_width()
    th = state.terminal_height
    items = sidebar_items(state)

    dim = Style.new(fg: :bright_black)
    cursor_bg = Style.new(bg: {55, 51, 84})

    rows =
      items
      |> Enum.with_index()
      |> Enum.map(fn {item, visual_idx} ->
        render_sidebar_item(item, visual_idx, state, sw, dim, cursor_bg)
      end)

    # Pad to full height
    pad_count = max(th - length(rows), 0)
    pad_rows = Enum.map(1..max(pad_count, 1), fn _ -> text(String.duplicate(" ", sw), nil) end)

    border_col = Style.new(fg: {50, 50, 55})

    sidebar_content = stack(:vertical, rows ++ pad_rows)

    # Right border via a thin column
    border =
      stack(
        :vertical,
        Enum.map(1..th, fn i -> text("│", %{border_col | fg: unique_rgb({50, 50, 55}, i)}) end)
      )

    stack(:horizontal, [sidebar_content, border])
  end

  defp sidebar_items(state) do
    flat_projects = flat_ordered_projects(state.projects)

    all_item = {:all, "All Todos", 0}
    blank = {:blank, "", 0}
    header = {:header, "Projects", 0}

    project_items =
      Enum.map(flat_projects, fn p ->
        depth = if p.parent_id, do: 1, else: 0
        {:project, p, depth}
      end)

    hint =
      if project_items == [] and state.focus == :sidebar do
        [{:hint, "press 'a' to create", 0}]
      else
        []
      end

    [all_item, blank, header] ++ project_items ++ hint
  end

  defp render_sidebar_item({:blank, _, _}, _vi, _state, sw, _dim, _cursor_bg) do
    text(String.duplicate(" ", sw), nil)
  end

  defp render_sidebar_item({:header, label, _}, vi, _state, sw, dim, _cursor_bg) do
    padded = String.pad_trailing("  " <> label, sw)
    text(padded, %{dim | fg: unique_rgb({100, 100, 100}, vi)})
  end

  defp render_sidebar_item({:hint, label, _}, vi, _state, sw, _dim, _cursor_bg) do
    padded = String.pad_trailing("    " <> label, sw)
    text(padded, Style.new(fg: unique_rgb({80, 80, 80}, vi)))
  end

  defp render_sidebar_item({:all, label, _}, vi, state, sw, _dim, cursor_bg) do
    cursor_idx = sidebar_cursor_to_visual(state, state.sidebar_cursor)
    is_cursor = vi == cursor_idx and state.focus == :sidebar
    is_selected = state.current_view == :all

    style = if is_selected, do: Style.new(fg: :white, attrs: [:bold]), else: nil

    padded = String.pad_trailing("  " <> label, sw)
    row = text(padded, style)
    if is_cursor, do: styled(row, cursor_bg), else: row
  end

  defp render_sidebar_item({:project, project, depth}, vi, state, sw, _dim, cursor_bg) do
    cursor_idx = sidebar_cursor_to_visual(state, state.sidebar_cursor)
    is_cursor = vi == cursor_idx and state.focus == :sidebar
    is_selected = state.current_view == {:project, project.id}

    indent = String.duplicate("  ", depth + 1)
    prefix = "# "

    {display, style} =
      cond do
        state.sidebar_mode == :insert and state.sidebar_editing_id == project.id ->
          {state.sidebar_editing_text <> "█", Style.new(fg: :green)}

        state.sidebar_mode == :confirm_delete and state.sidebar_confirm_project_id == project.id ->
          {"Delete? y/n", Style.new(fg: :red)}

        is_selected ->
          {project.name, Style.new(fg: :white, attrs: [:bold])}

        true ->
          {project.name, nil}
      end

    label = indent <> prefix <> display
    padded = String.pad_trailing(label, sw)
    # Truncate if too long
    padded = String.slice(padded, 0, sw)
    row = text(padded, style)
    if is_cursor, do: styled(row, cursor_bg), else: row
  end

  defp sidebar_cursor_to_visual(state, cursor) do
    # Items: 0=All, 1=blank, 2=header, 3+=projects
    # Cursor 0 = All Todos (visual 0)
    # Cursor 1+ = project at index cursor-1 (visual 3+)
    flat = flat_ordered_projects(state.projects)

    if cursor == 0 do
      0
    else
      project_idx = cursor - 1

      if project_idx < length(flat) do
        3 + project_idx
      else
        0
      end
    end
  end

  defdelegate flat_ordered_projects(projects), to: Helpers

  # --- Main content ---

  def render_main(state, available_width) do
    content_w = content_width(available_width)
    left_pad = div(available_width - content_w, 2)
    pad_str = String.duplicate(" ", max(left_pad, 0))

    top_pad = blank_rows(Home.pad_y_top())
    list_rows = render_list(state, content_w, pad_str)
    vh = max(state.terminal_height - Home.pad_y_top() - Home.bottom_reserved(), 1)

    max_offset = max(length(list_rows) - vh, 0)
    scroll = min(state.scroll_offset, max_offset)

    visible_rows =
      list_rows
      |> Enum.drop(scroll)
      |> Enum.take(vh)

    pad_count = vh - length(visible_rows)
    visible_pad = if pad_count > 0, do: blank_rows(pad_count), else: []

    bottom_rows = render_bottom(state, content_w, pad_str)

    all = top_pad ++ visible_rows ++ visible_pad ++ bottom_rows
    stack(:vertical, all)
  end

  defp unique_rgb({r, g, b}, idx) when b >= 255, do: {r, g, b - rem(idx, 2)}
  defp unique_rgb({r, g, b}, idx), do: {r, g, b + rem(idx, 2)}

  def content_width(available_width) do
    max(trunc(available_width * 0.6), 20)
  end

  def blank_rows(0), do: []
  def blank_rows(n), do: Enum.map(1..n, fn _ -> text("", nil) end)

  def render_list(state, content_w, pad_str) do
    {disp_active, disp_completed} = Helpers.display_todos(state)

    has_project_todos = state.current_view == :all and Enum.any?(disp_active, & &1.source)
    show_sections = has_project_todos and state.mode not in [:search, :search_nav]

    active_rows =
      if show_sections do
        render_sectioned_active(disp_active, state, content_w, pad_str)
      else
        title = view_title(state)
        header = [render_section_header(title, "Created", content_w, pad_str)]
        spacing = blank_rows(1)

        rows =
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

        header ++ spacing ++ rows
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

    active_rows ++
      section_spacing ++ completed_header ++ spacing_above_completed ++ completed_rows
  end

  defp render_sectioned_active(disp_active, state, content_w, pad_str) do
    active_with_idx = Enum.with_index(disp_active)

    # Group by source, maintaining order
    groups =
      active_with_idx
      |> Enum.chunk_while(
        nil,
        fn {todo, idx}, acc ->
          source = todo.source
          case acc do
            nil -> {:cont, {source, [{todo, idx}]}}
            {^source, items} -> {:cont, {source, items ++ [{todo, idx}]}}
            {prev_source, items} -> {:cont, {prev_source, items}, {source, [{todo, idx}]}}
          end
        end,
        fn
          nil -> {:cont, nil}
          acc -> {:cont, acc, nil}
        end
      )
      |> Enum.reject(&is_nil/1)

    groups
    |> Enum.with_index()
    |> Enum.flat_map(fn {{source, items}, group_idx} ->
      label =
        if source do
          project = Enum.find(state.projects, &(&1.id == source))
          if project, do: Doer.Home.SidebarUpdate.section_label(state.projects, project), else: "Todos"
        else
          "Todos"
        end

      spacing = if group_idx > 0, do: blank_rows(1), else: []
      header = [render_section_header(label, "Created", content_w, pad_str, group_idx)]
      header_spacing = blank_rows(1)

      rows =
        Enum.flat_map(items, fn {todo, idx} ->
          render_todo_row(todo, idx, state, false, content_w, pad_str)
        end)

      spacing ++ header ++ header_spacing ++ rows
    end)
  end

  defp view_title(state) do
    case state.current_view do
      :all ->
        "Todos"

      {:project, id} ->
        case Enum.find(state.projects, &(&1.id == id)) do
          nil -> "Todos"
          project -> "# #{project.name} todos"
        end
    end
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

    age_str = Todo.age_label(todo)

    completed_age_str =
      if is_completed and todo.completed_at, do: Todo.completed_label(todo), else: nil

    right_col = "  " <> String.pad_leading(age_str, 7)

    right_col =
      if completed_age_str,
        do: right_col <> "  " <> String.pad_leading(completed_age_str, 9),
        else: right_col

    right_w = String.length(right_col)
    text_area_w = max(content_w - Home.prefix_w() - right_w, 10)
    display_text = if is_editing, do: state.editing_text <> "█", else: todo.text
    lines = Helpers.wrap_text(display_text, text_area_w)

    indicator = if is_selected, do: "▎ ", else: "  "
    checkbox = if is_completed, do: "◉ ", else: "◯ "
    prefix = indicator <> checkbox
    continuation_prefix = String.duplicate(" ", Home.prefix_w())

    text_style =
      cond do
        is_editing -> Style.new(fg: :green)
        is_completed -> Style.new(fg: unique_rgb({80, 80, 80}, idx), attrs: [:strikethrough])
        is_cursor -> Style.new(fg: :white, attrs: [:bold])
        true -> nil
      end

    right_style = Style.new(fg: unique_rgb({140, 140, 140}, idx))
    cursor_bg = if is_cursor and not is_editing, do: Style.new(bg: {55, 51, 84}), else: nil

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

    total = length(state.todos)
    done = Enum.count(state.todos, & &1.done)
    count_text = "#{done}/#{total} completed"
    count_w = String.length(count_text)

    hint_text = if state.mode == :normal and not state.show_help, do: "? for help", else: ""
    hint_w = String.length(hint_text)

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
      "G/g        end / start",
      "ctrl+d/u   half page down/up",
      "\\          toggle sidebar",
      "Tab        switch focus",
      "?          toggle help",
      "q          quit",
      "",
      "Sidebar",
      "",
      "a          add project",
      "s          add subproject",
      "e/i        rename project",
      "d          delete project",
      "J/K        reorder projects",
      "Enter/l    select project",
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
