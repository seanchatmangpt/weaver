# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeEmptyRegistryTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "empty registry is refused before execution" do
    assert {:error, %Refusal{type: :empty_registry}} = Runtime.plan("check", "")
  end
end
