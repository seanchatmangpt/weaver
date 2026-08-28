# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeInvalidOperationTypeTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "non-string operation is typed invalid plan input" do
    assert {:error, %Refusal{type: :invalid_plan_input}} = Runtime.plan(:check, "fixtures/registry")
  end
end
