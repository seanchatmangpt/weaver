# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeNulRegistryTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "registry containing NUL is refused before execution" do
    assert {:error, %Refusal{type: :invalid_registry}} = Runtime.plan("check", "registry\0bad")
  end
end
