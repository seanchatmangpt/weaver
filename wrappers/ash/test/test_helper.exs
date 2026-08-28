# SPDX-License-Identifier: Apache-2.0

exclude =
  if System.get_env("RUN_WEAVER_INTEGRATION") == "1" do
    []
  else
    [:integration]
  end

ExUnit.start(exclude: exclude)
