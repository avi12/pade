$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class Msal {
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate void Done(IntPtr result, IntPtr data);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate IntPtr Startup();
  [UnmanagedFunctionPointer(CallingConvention.Cdecl, CharSet = CharSet.Unicode)] delegate IntPtr Create(string clientId, string authority, out IntPtr parameters);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl, CharSet = CharSet.Unicode)] delegate IntPtr SetValue(IntPtr parameters, string value);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl, CharSet = CharSet.Unicode)] delegate IntPtr SetPair(IntPtr parameters, string key, string value);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl, CharSet = CharSet.Unicode)] delegate IntPtr SignIn(IntPtr parameters, string correlationId, Done done, IntPtr data, out IntPtr async);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl, CharSet = CharSet.Unicode)] delegate IntPtr GetToken(IntPtr result, StringBuilder buffer, ref int size);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate IntPtr GetError(IntPtr result, out IntPtr error);

  [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)] static extern IntPtr LoadLibraryW(string path);
  [DllImport("kernel32")] static extern IntPtr GetProcAddress(IntPtr module, string name);

  static T Fn<T>(IntPtr module, string name) where T : class {
    return Marshal.GetDelegateForFunctionPointer(GetProcAddress(module, name), typeof(T)) as T;
  }

  public static string Run(string dll) {
    IntPtr module = LoadLibraryW(dll);
    if (module == IntPtr.Zero) return "ERR cannot load " + dll;
    Fn<Startup>(module, "MSALRUNTIME_Startup")();
    IntPtr parameters;
    Fn<Create>(module, "MSALRUNTIME_CreateAuthParameters")("c0ab8ce9-e9a0-42e7-b064-33d422df41f1", "https://login.microsoftonline.com/consumers", out parameters);
    Fn<SetValue>(module, "MSALRUNTIME_SetRequestedScopes")(parameters, "https://substrate.office.com/sydney/v2/.default");
    Fn<SetValue>(module, "MSALRUNTIME_SetRedirectUri")(parameters, "https://login.microsoftonline.com/common/oauth2/nativeclient");
    Fn<SetPair>(module, "MSALRUNTIME_SetAdditionalParameter")(parameters, "msal_request_type", "consumer_passthrough");

    IntPtr result = IntPtr.Zero;
    var finished = new ManualResetEvent(false);
    Done done = (r, data) => { result = r; finished.Set(); };
    IntPtr async;
    if (Fn<SignIn>(module, "MSALRUNTIME_SignInSilentlyAsync")(parameters, Guid.NewGuid().ToString(), done, IntPtr.Zero, out async) != IntPtr.Zero) return "ERR the sign-in did not start";
    if (!finished.WaitOne(20000)) return "ERR the sign-in took more than 20 s";
    IntPtr error;
    Fn<GetError>(module, "MSALRUNTIME_GetError")(result, out error);
    if (error != IntPtr.Zero) return "ERR Windows is not signed in to a personal Microsoft account";
    int size = 0;
    Fn<GetToken>(module, "MSALRUNTIME_GetAccessToken")(result, null, ref size);
    var token = new StringBuilder(size);
    Fn<GetToken>(module, "MSALRUNTIME_GetAccessToken")(result, token, ref size);
    GC.KeepAlive(done);
    return token.ToString();
  }
}
'@
$out = [Msal]::Run($env:PADE_MSALRUNTIME)
if ($out.StartsWith('ERR ')) { [Console]::Error.WriteLine($out.Substring(4)); exit 1 }
[Console]::Out.Write($out)
