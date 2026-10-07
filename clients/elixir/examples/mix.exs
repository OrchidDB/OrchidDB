defmodule OrchidDB.Example.MixProject do
  use Mix.Project

  def project do
    [
      app: :orchiddb_example,
      version: "0.2.1",
      elixir: "~> 1.15",
      deps: [{:orchiddb, path: ".."}, {:adbc, "~> 0.12"}]
    ]
  end

  def application, do: [extra_applications: [:logger]]
end
