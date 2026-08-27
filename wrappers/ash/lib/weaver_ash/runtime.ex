# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.Refusal do
  @moduledoc "Typed fail-closed refusal emitted before Weaver actuation."
  defexception [:type, :message, details: %{}]
end

defmodule WeaverAsh.Plan do
  @moduledoc false
  @enforce_keys [:operation, :registry, :executable, :args, :authority]
  defstruct [:operation, :registry, :executable, :args, :authority, :id]
end

defmodule WeaverAsh.Receipt do
  @moduledoc "Receipt binding subject, executable identity, argv, consequence, and replay data."

  @enforce_keys [
    :status,
    :standing,
    :operation,
    :registry,
    :subject_sha256,
    :executable,
    :executable_sha256,
    :argv,
    :started_at,
    :completed_at,
    :duration_us,
    :exit_status,
    :output_sha256,
    :output_bytes
  ]

  defstruct [
    :status,
    :standing,
    :operation,
    :registry,
    :subject_sha256,
    :executable,
    :executable_sha256,
    :argv,
    :started_at,
    :completed_at,
    :duration_us,
    :exit_status,
    :output_sha256,
    :output_bytes,
    :error
  ]
end

defmodule WeaverAsh.Runtime do
  @moduledoc """
  The exclusive DO boundary for the Ash wrapper.

  Ontology and ggen output manufacture an operation contract, but they never
  receive ambient process authority. This module independently fences the
  generated contract to the three local, non-server Weaver registry commands
  admitted by v0.1.0 and executes with `System.cmd/3` argv semantics only.
  """

  alias WeaverAsh.Generated.OperationContract
  alias WeaverAsh.{Plan, Receipt, Refusal}

  @authority "WEAVER_EXECUTE_RECEIPTED"
  @fenced_pairs MapSet.new([{"check", ""}, {"stats", ""}, {"package", "json"}])

  @spec plan(String.t(), String.t(), keyword()) :: {:ok, Plan.t()} | {:error, Refusal.t()}
  def plan(operation, registry, options \\ [])

  def plan(operation, registry, options)
      when is_binary(operation) and is_binary(registry) and is_list(options) do
    with {:ok, spec} <- fetch_operation(operation),
         :ok <- admit_spec(spec),
         {:ok, registry} <- admit_local_registry(registry),
         {:ok, id} <- admit_id(spec, Keyword.get(options, :id)),
         {:ok, executable} <- admit_executable(Keyword.get(options, :executable, "weaver")) do
      args = build_args(spec, registry, id)

      {:ok,
       %Plan{
         operation: operation,
         registry: registry,
         executable: executable,
         args: args,
         authority: @authority,
         id: id
       }}
    end
  end

  def plan(operation, registry, options) do
    {:error,
     refusal(:invalid_plan_input, "operation and registry must be strings and options a keyword list", %{
       operation: inspect(operation),
       registry: inspect(registry),
       options: inspect(options)
     })}
  end

  @spec execute(Plan.t()) :: {:ok, Receipt.t()} | {:error, Receipt.t() | Refusal.t()}
  def execute(%Plan{} = plan) do
    with :ok <- revalidate_plan(plan),
         {:ok, executable} <- resolve_executable(plan.executable),
         {:ok, executable_sha256} <- sha256_file(executable),
         {:ok, subject_sha256} <- subject_sha256(plan.registry) do
      actuate(plan, executable, executable_sha256, subject_sha256)
    end
  end

  def execute(other) do
    {:error,
     refusal(:invalid_plan, "DO accepts only a WeaverAsh.Plan produced by the admitted planner", %{
       value: inspect(other)
     })}
  end

  defp fetch_operation(operation) do
    case OperationContract.fetch(operation) do
      {:ok, spec} -> {:ok, spec}
      :error -> {:error, refusal(:unsupported_operation, "operation is not present in the ggen contract", %{operation: operation})}
    end
  end

  defp admit_spec(%{subcommand: subcommand, target: target, authority: @authority}) do
    if MapSet.member?(@fenced_pairs, {subcommand, target}) do
      :ok
    else
      {:error,
       refusal(:contract_outside_runtime_fence, "generated contract cannot widen DO authority", %{
         subcommand: subcommand,
         target: target
       })}
    end
  end

  defp admit_spec(spec) do
    {:error,
     refusal(:invalid_authority, "generated operation lacks the required receipted-execution authority", %{
       spec: inspect(spec)
     })}
  end

  defp admit_local_registry(registry) do
    cond do
      registry == "" ->
        {:error, refusal(:empty_registry, "registry path cannot be empty")}

      String.contains?(registry, "://") or String.starts_with?(registry, "git@") ->
        {:error,
         refusal(:remote_registry_not_admitted, "v0.1.0 only actuates against locally hashable registry subjects", %{
           registry: registry
         })}

      String.contains?(registry, <<0>>) ->
        {:error, refusal(:invalid_registry, "registry path contains a NUL byte")}

      true ->
        {:ok, Path.expand(registry)}
    end
  end

  defp admit_id(%{allows_id: "true"}, nil), do: {:ok, nil}
  defp admit_id(%{allows_id: "true"}, id) when is_binary(id) and id != "", do: {:ok, id}
  defp admit_id(%{allows_id: "false"}, nil), do: {:ok, nil}

  defp admit_id(%{allows_id: "false"}, id) do
    {:error, refusal(:id_not_admitted, "this operation does not accept --id", %{id: inspect(id)})}
  end

  defp admit_id(_spec, id) do
    {:error, refusal(:invalid_id, "id must be a non-empty string when supplied", %{id: inspect(id)})}
  end

  defp admit_executable(executable) when is_binary(executable) and executable != "" do
    if String.contains?(executable, <<0>>) do
      {:error, refusal(:invalid_executable, "executable contains a NUL byte")}
    else
      {:ok, executable}
    end
  end

  defp admit_executable(executable) do
    {:error, refusal(:invalid_executable, "executable must be a non-empty string", %{executable: inspect(executable)})}
  end

  defp build_args(spec, registry, id) do
    args = ["registry", spec.subcommand, "-r", registry]
    args = if spec.target == "", do: args, else: args ++ ["-t", spec.target]
    if id, do: args ++ ["--id", id], else: args
  end

  defp revalidate_plan(%Plan{authority: @authority} = plan) do
    options = [executable: plan.executable]
    options = if plan.id, do: Keyword.put(options, :id, plan.id), else: options

    case plan(plan.operation, plan.registry, options) do
      {:ok, expected} when expected.args == plan.args -> :ok
      {:ok, _expected} -> {:error, refusal(:plan_tampered, "plan argv no longer matches the admitted contract")}
      {:error, refusal} -> {:error, refusal}
    end
  end

  defp revalidate_plan(_plan),
    do: {:error, refusal(:plan_tampered, "plan authority no longer matches the admitted contract")}

  defp resolve_executable(executable) do
    case System.find_executable(executable) do
      nil -> {:error, refusal(:executable_not_found, "Weaver executable was not found before actuation", %{executable: executable})}
      path -> {:ok, Path.expand(path)}
    end
  end

  defp subject_sha256(path) do
    case File.lstat(path) do
      {:ok, %File.Stat{type: :regular}} -> sha256_file(path)
      {:ok, %File.Stat{type: :directory}} -> sha256_directory(path)
      {:ok, %File.Stat{type: type}} -> {:error, refusal(:unsupported_registry_subject, "registry subject must be a regular file or directory", %{type: type})}
      {:error, reason} -> {:error, refusal(:registry_identity_unavailable, "registry subject could not be identified before actuation", %{reason: reason, registry: path})}
    end
  end

  defp sha256_directory(root) do
    paths = Path.wildcard(Path.join(root, "**/*"), match_dot: true)

    case Enum.find(paths, fn path -> match?({:ok, %File.Stat{type: :symlink}}, File.lstat(path)) end) do
      nil -> hash_regular_tree(root, paths)
      symlink -> {:error, refusal(:symlink_registry_not_admitted, "registry trees containing symlinks are refused until symlink identity semantics are admitted", %{symlink: symlink})}
    end
  rescue
    error -> {:error, refusal(:registry_identity_unavailable, "registry tree hashing failed before actuation", %{error: Exception.message(error)})}
  end

  defp hash_regular_tree(root, paths) do
    files =
      paths
      |> Enum.filter(fn path -> match?({:ok, %File.Stat{type: :regular}}, File.lstat(path)) end)
      |> Enum.sort()

    context = :crypto.hash_init(:sha256)

    context =
      Enum.reduce(files, context, fn path, context ->
        relative = path |> Path.relative_to(root) |> String.replace("\\", "/")
        context = :crypto.hash_update(context, ["file\0", relative, "\0"])

        File.stream!(path, [], 65_536)
        |> Enum.reduce(context, fn chunk, acc -> :crypto.hash_update(acc, chunk) end)
        |> :crypto.hash_update("\0")
      end)

    {:ok, context |> :crypto.hash_final() |> hex()}
  end

  defp sha256_file(path) do
    context =
      File.stream!(path, [], 65_536)
      |> Enum.reduce(:crypto.hash_init(:sha256), fn chunk, context ->
        :crypto.hash_update(context, chunk)
      end)

    {:ok, context |> :crypto.hash_final() |> hex()}
  rescue
    error -> {:error, refusal(:identity_hash_failed, "subject identity could not be hashed before actuation", %{path: path, error: Exception.message(error)})}
  end

  defp actuate(plan, executable, executable_sha256, subject_sha256) do
    started_at = DateTime.utc_now()
    started_native = System.monotonic_time()

    try do
      {output, exit_status} = System.cmd(executable, plan.args, stderr_to_stdout: true)

      receipt =
        receipt(
          plan,
          executable,
          executable_sha256,
          subject_sha256,
          started_at,
          started_native,
          output,
          exit_status,
          nil
        )

      if exit_status == 0, do: {:ok, receipt}, else: {:error, receipt}
    rescue
      error ->
        output = Exception.format(:error, error, __STACKTRACE__)

        {:error,
         receipt(
           plan,
           executable,
           executable_sha256,
           subject_sha256,
           started_at,
           started_native,
           output,
           nil,
           Exception.message(error)
         )}
    catch
      kind, reason ->
        output = Exception.format(kind, reason, __STACKTRACE__)

        {:error,
         receipt(
           plan,
           executable,
           executable_sha256,
           subject_sha256,
           started_at,
           started_native,
           output,
           nil,
           output
         )}
    end
  end

  defp receipt(plan, executable, executable_sha256, subject_sha256, started_at, started_native, output, exit_status, error) do
    completed_at = DateTime.utc_now()
    duration_us = System.convert_time_unit(System.monotonic_time() - started_native, :native, :microsecond)

    %Receipt{
      status: if(exit_status == 0, do: "ALIVE", else: "PARTIAL_ALIVE"),
      standing: "executed_receipted",
      operation: plan.operation,
      registry: plan.registry,
      subject_sha256: subject_sha256,
      executable: executable,
      executable_sha256: executable_sha256,
      argv: [executable | plan.args],
      started_at: DateTime.to_iso8601(started_at),
      completed_at: DateTime.to_iso8601(completed_at),
      duration_us: duration_us,
      exit_status: exit_status,
      output_sha256: output |> :crypto.hash(:sha256) |> hex(),
      output_bytes: byte_size(output),
      error: error
    }
  end

  defp refusal(type, message, details \\ %{}),
    do: %Refusal{type: type, message: message, details: details}

  defp hex(binary), do: Base.encode16(binary, case: :lower)
end
