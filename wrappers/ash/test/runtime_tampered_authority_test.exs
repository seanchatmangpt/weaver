# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeTamperedAuthorityTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Plan, Refusal, Runtime}

  test "execute refuses a plan whose authority was tampered" do
    assert {:ok, plan} = Runtime.plan("check", "fixtures/registry")
    tampered = %Plan{plan | authority: "AMBIENT_DO"}
    assert {:error, %Refusal{type: :plan_tampered}} = Runtime.execute(tampered)
  end
end
