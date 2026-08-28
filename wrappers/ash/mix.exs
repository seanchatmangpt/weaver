# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.MixProject do
  use Mix.Project

  @ash_r2rml_ref "067954ad406fd637fd47646bdb10c4580809c79d"

  def project do
    [
      app: :weaver_ash,
      version: "0.1.0",
      elixir: "~> 1.18",
      start_permanent: Mix.env() == :prod,
      deps: deps()
    ]
  end

  def application do
    [extra_applications: [:logger, :crypto]]
  end

  defp deps do
    [
      {:ash, "== 3.29.3"},
      {:reactor, "~> 1.0"},
      {:jason, "~> 1.4"},
      {:ash_r2rml,
       github: "seanchatmangpt/ash_r2rml",
       ref: @ash_r2rml_ref}
    ]
  end
end
