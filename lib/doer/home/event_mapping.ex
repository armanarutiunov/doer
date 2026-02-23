defmodule Doer.Home.EventMapping do
  alias TermUI.Event

  # Resize — global
  def event_to_msg(%Event.Resize{width: w, height: h}, _state),
    do: {:msg, {:resize, w, h}}

  # Help — global (only when sidebar is in normal mode too)
  def event_to_msg(%Event.Key{key: "?", modifiers: []}, %{mode: :normal, sidebar_mode: :normal}),
    do: {:msg, :toggle_help}

  def event_to_msg(%Event.Key{key: :escape}, %{show_help: true}),
    do: {:msg, :toggle_help}

  # Quit — global, both modes normal
  def event_to_msg(%Event.Key{key: "q", modifiers: []}, %{mode: :normal, show_help: false, sidebar_mode: :normal}),
    do: {:msg, :quit}

  # Toggle sidebar — global, both modes normal
  def event_to_msg(%Event.Key{key: "\\", modifiers: []}, %{mode: :normal, show_help: false, sidebar_mode: :normal}),
    do: {:msg, :toggle_sidebar}

  # Switch focus — Tab, both modes normal
  def event_to_msg(%Event.Key{key: :tab, modifiers: []}, %{mode: :normal, show_help: false, sidebar_open: true, sidebar_mode: :normal}),
    do: {:msg, :switch_focus}

  # Sidebar-focused dispatch
  def event_to_msg(event, %{focus: :sidebar, show_help: false} = state),
    do: Doer.Home.SidebarEventMapping.event_to_msg(event, state)

  # --- Main focus ---

  # Normal mode — ctrl combos first
  def event_to_msg(%Event.Key{key: "d", modifiers: [:ctrl]}, %{mode: :normal, show_help: false}),
    do: {:msg, :half_page_down}

  def event_to_msg(%Event.Key{key: "u", modifiers: [:ctrl]}, %{mode: :normal, show_help: false}),
    do: {:msg, :half_page_up}

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

  def event_to_msg(%Event.Key{key: key}, %{mode: :insert})
      when is_binary(key) and byte_size(key) == 1,
      do: {:msg, {:type_char, key}}

  # Visual mode — ctrl combos first
  def event_to_msg(%Event.Key{key: key, modifiers: [:ctrl]}, %{mode: :visual})
      when key in ["j", :down],
      do: {:msg, :move_selected_down}

  def event_to_msg(%Event.Key{key: key, modifiers: [:ctrl]}, %{mode: :visual})
      when key in ["k", :up],
      do: {:msg, :move_selected_up}

  def event_to_msg(%Event.Key{key: "J", modifiers: []}, %{mode: :visual}),
    do: {:msg, :move_selected_down}

  def event_to_msg(%Event.Key{key: "K", modifiers: []}, %{mode: :visual}),
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

  def event_to_msg(%Event.Key{key: key}, %{mode: :search})
      when is_binary(key) and byte_size(key) == 1,
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
end
