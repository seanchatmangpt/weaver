# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAshTest do
  use ExUnit.Case, async: true

  alias WeaverAsh.Generated.OperationContract
  alias WeaverAsh.{Plan, Receipt, Refusal, Runtime}

  test "ggen projection contains exactly the three admitted operations" do
    assert OperationContract.all() |> Map.keys() |> Enum.sort() == [
             "check",
             "package_json",
             "stats"
           ]

    assert {:ok, %{subcommand: "check", authority: "WEAVER_EXECUTE_RECEIPTED"}} =
             OperationContract.fetch("check")

    assert :error = OperationContract.fetch("serve")
  end

  test "Ash resources compile through AshR2RML into a valid semantic bundle" do
    assert {:ok, %AshR2RML.Mapping.Bundle{} = bundle} =
             AshR2RML.compile([WeaverAsh.Registry, WeaverAsh.ExecutionReceipt])

    assert :ok = AshR2RML.Mapping.validate(bundle)

    registry_mapping = AshR2RML.Resource.Info.mapping!(WeaverAsh.Registry)
    receipt_mapping = AshR2RML.Resource.Info.mapping!(WeaverAsh.ExecutionReceipt)

    assert registry_mapping.class_iris == ["https://opentelemetry.io/weaver/ash#WeaverRegistry"]
    assert registry_mapping.logical_table.table_name == "weaver_registries"
    assert receipt_mapping.class_iris == ["https://opentelemetry.io/weaver/ash#Receipt"]
    assert receipt_mapping.logical_table.table_name == "weaver_execution_receipts"
  end

  test "planner manufactures argv without a shell" do
    registry = Path.expand("fixtures/registry")

    assert {:ok,
            %Plan{
              operation: "package_json",
              registry: ^registry,
              args: ["registry", "package", "-r", ^registry, "-t", "json", "--id", "otel"]
            }} = Runtime.plan("package_json", "fixtures/registry", id: "otel")
  end

  test "raw, server, and remote subjects are typed refusals" do
    assert {:error, %Refusal{type: :unsupported_operation}} =
             Runtime.plan("serve", "fixtures/registry")

    assert {:error, %Refusal{type: :unsupported_operation}} =
             Runtime.plan("registry check; rm -rf /", "fixtures/registry")

    assert {:error, %Refusal{type: :remote_registry_not_admitted}} =
             Runtime.plan("check", "https://github.com/open-telemetry/semantic-conventions.git[model]")
  end

  test "operation-specific flags cannot cross the generated contract" do
    assert {:error, %Refusal{type: :id_not_admitted}} =
             Runtime.plan("check", "fixtures/registry", id: "unexpected")
  end

  test "tampered plans are refused before process lookup or actuation" do
    assert {:ok, plan} = Runtime.plan("check", "fixtures/registry")
    tampered = %{plan | args: ["serve"]}

    assert {:error, %Refusal{type: :plan_tampered}} = Runtime.execute(tampered)
  end

  @tag :integration
  test "Reactor executes the exact built Weaver subject and emits a replay receipt" do
    executable = System.fetch_env!("WEAVER_BIN")
    registry = System.fetch_env!("WEAVER_REGISTRY")

    assert {:ok,
            %Receipt{
              status: "ALIVE",
              standing: "executed_receipted",
              exit_status: 0,
              subject_sha256: subject_sha256,
              executable_sha256: executable_sha256,
              output_sha256: output_sha256
            } = receipt} = WeaverAsh.run("check", registry, executable: executable)

    assert byte_size(subject_sha256) == 64
    assert byte_size(executable_sha256) == 64
    assert byte_size(output_sha256) == 64
    assert receipt.argv == [receipt.executable, "registry", "check", "-r", Path.expand(registry)]

    File.mkdir_p!("tmp")

    receipt
    |> Map.from_struct()
    |> Jason.encode!(pretty: true)
    |> then(&File.write!("tmp/integration-receipt.json", &1))
  end
end
