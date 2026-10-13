-module(mtparser_tools_ffi).
-export([spawn_fun/1, send_msg/2, recv_msg/1]).

%% Erlang FFI for the integration harness: process spawning and mailbox
%% receive (Gleam has no `receive` expression, so it lives here).
%% Only compiled for the Erlang target; the JavaScript build ignores it.

spawn_fun(Fun) ->
    erlang:spawn(Fun).

send_msg(Pid, Msg) ->
    erlang:send(Pid, Msg).

recv_msg(Timeout) ->
    receive
        Msg -> {ok, Msg}
    after Timeout -> {error, nil}
    end.
