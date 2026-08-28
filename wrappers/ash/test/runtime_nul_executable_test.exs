# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeNulExecutableTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "executable containing NUL is refused before execution" do
    assert {:error, %Refusal{type: :invalid_executable}} = Runtime.plan("check", "fixtures/registry", executable: "weaver\0bad")
  end
end
