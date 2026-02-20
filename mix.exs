defmodule Doer.MixProject do
  use Mix.Project

  def project do
    [
      app: :doer,
      version: "0.1.0",
      elixir: "~> 1.15",
      start_permanent: Mix.env() == :prod,
      deps: deps()
    ]
  end

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp deps do
    [
      {:term_ui, "~> 0.2.0"},
      {:jason, "~> 1.4"}
    ]
  end
end
