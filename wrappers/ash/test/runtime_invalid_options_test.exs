# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeInvalidOptionsTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "non-keyword options are typed invalid plan input" do
    assert {:error, %Refusal{type: :invalid_plan_input}} = Runtime.plan("check", "fixtures/registry", %{})
  end
end
