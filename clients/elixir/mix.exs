defmodule OrchidDB.MixProject do
  use Mix.Project

  def project do
    [
      app: :orchiddb,
      version: "0.4.0",
      elixir: "~> 1.15",
      compilers: [:elixir_make] ++ Mix.compilers(),
      make_clean: ["clean"],
      description: "Graph query execution on caller-owned database connections",
      package: [
        licenses: ["GPL-3.0-only"],
        links: %{"GitHub" => "https://github.com/OrchidDB/OrchidDB"},
        files: [
          "lib",
          "c_src",
          "Makefile",
          "mix.exs",
          "README.md",
          "LICENSE.md",
          "CORE_REVISION"
        ]
      ],
      deps: [
        {:jason, "~> 1.4"},
        {:elixir_make, "~> 0.9", runtime: false},
        {:adbc, "~> 0.12", optional: true}
      ]
    ]
  end

  def application, do: [extra_applications: [:logger]]
end
