# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeEmptyExecutableTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "empty executable identity is refused before execution" do
    assert {:error, %Refusal{type: :invalid_executable}} = Runtime.plan("check", "fixtures/registry", executable: "")
  end
end
