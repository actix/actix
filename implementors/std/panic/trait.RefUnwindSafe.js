(function() {var implementors = {};
implementors["actix"] = [{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/prelude/enum.ActorState.html\" title=\"enum actix::prelude::ActorState\">ActorState</a>","synthetic":true,"types":["actix::actor::ActorState"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/prelude/enum.Running.html\" title=\"enum actix::prelude::Running\">Running</a>","synthetic":true,"types":["actix::actor::Running"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.SpawnHandle.html\" title=\"struct actix::prelude::SpawnHandle\">SpawnHandle</a>","synthetic":true,"types":["actix::actor::SpawnHandle"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.Context.html\" title=\"struct actix::prelude::Context\">Context</a>&lt;A&gt;","synthetic":true,"types":["actix::context::Context"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/struct.ContextParts.html\" title=\"struct actix::dev::ContextParts\">ContextParts</a>&lt;A&gt;","synthetic":true,"types":["actix::contextimpl::ContextParts"]},{"text":"impl&lt;A, C&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/struct.ContextFut.html\" title=\"struct actix::dev::ContextFut\">ContextFut</a>&lt;A, C&gt;","synthetic":true,"types":["actix::contextimpl::ContextFut"]},{"text":"impl&lt;M&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.MessageResult.html\" title=\"struct actix::prelude::MessageResult\">MessageResult</a>&lt;M&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;M as <a class=\"trait\" href=\"actix/prelude/trait.Message.html\" title=\"trait actix::prelude::Message\">Message</a>&gt;::<a class=\"type\" href=\"actix/prelude/trait.Message.html#associatedtype.Result\" title=\"type actix::prelude::Message::Result\">Result</a>: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::handler::MessageResult"]},{"text":"impl&lt;A, T&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.AtomicResponse.html\" title=\"struct actix::prelude::AtomicResponse\">AtomicResponse</a>&lt;A, T&gt;","synthetic":true,"types":["actix::handler::AtomicResponse"]},{"text":"impl&lt;I&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.Response.html\" title=\"struct actix::prelude::Response\">Response</a>&lt;I&gt;","synthetic":true,"types":["actix::handler::Response"]},{"text":"impl&lt;A, I&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.ActorResponse.html\" title=\"struct actix::prelude::ActorResponse\">ActorResponse</a>&lt;A, I&gt;","synthetic":true,"types":["actix::handler::ActorResponse"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.Supervisor.html\" title=\"struct actix::prelude::Supervisor\">Supervisor</a>&lt;A&gt;","synthetic":true,"types":["actix::supervisor::Supervisor"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/channel/struct.AddressSender.html\" title=\"struct actix::dev::channel::AddressSender\">AddressSender</a>&lt;A&gt;","synthetic":true,"types":["actix::address::channel::AddressSender"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/channel/struct.AddressReceiver.html\" title=\"struct actix::dev::channel::AddressReceiver\">AddressReceiver</a>&lt;A&gt;","synthetic":true,"types":["actix::address::channel::AddressReceiver"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/struct.Envelope.html\" title=\"struct actix::dev::Envelope\">Envelope</a>&lt;A&gt;","synthetic":true,"types":["actix::address::envelope::Envelope"]},{"text":"impl&lt;T&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/prelude/enum.SendError.html\" title=\"enum actix::prelude::SendError\">SendError</a>&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::address::SendError"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/prelude/enum.MailboxError.html\" title=\"enum actix::prelude::MailboxError\">MailboxError</a>","synthetic":true,"types":["actix::address::MailboxError"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.Addr.html\" title=\"struct actix::prelude::Addr\">Addr</a>&lt;A&gt;","synthetic":true,"types":["actix::address::Addr"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/struct.WeakAddr.html\" title=\"struct actix::WeakAddr\">WeakAddr</a>&lt;A&gt;","synthetic":true,"types":["actix::address::WeakAddr"]},{"text":"impl&lt;M&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/prelude/struct.Recipient.html\" title=\"struct actix::prelude::Recipient\">Recipient</a>&lt;M&gt;","synthetic":true,"types":["actix::address::Recipient"]},{"text":"impl&lt;M&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/struct.WeakRecipient.html\" title=\"struct actix::WeakRecipient\">WeakRecipient</a>&lt;M&gt;","synthetic":true,"types":["actix::address::WeakRecipient"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/dev/struct.Mailbox.html\" title=\"struct actix::dev::Mailbox\">Mailbox</a>&lt;A&gt;","synthetic":true,"types":["actix::mailbox::Mailbox"]},{"text":"impl&lt;T&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/mocker/struct.Mocker.html\" title=\"struct actix::actors::mocker::Mocker\">Mocker</a>&lt;T&gt;","synthetic":true,"types":["actix::actors::mocker::Mocker"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/resolver/struct.Resolve.html\" title=\"struct actix::actors::resolver::Resolve\">Resolve</a>","synthetic":true,"types":["actix::actors::resolver::Resolve"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/resolver/struct.Connect.html\" title=\"struct actix::actors::resolver::Connect\">Connect</a>","synthetic":true,"types":["actix::actors::resolver::Connect"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/resolver/struct.ConnectAddr.html\" title=\"struct actix::actors::resolver::ConnectAddr\">ConnectAddr</a>","synthetic":true,"types":["actix::actors::resolver::ConnectAddr"]},{"text":"impl !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/actors/resolver/enum.ResolverError.html\" title=\"enum actix::actors::resolver::ResolverError\">ResolverError</a>","synthetic":true,"types":["actix::actors::resolver::ResolverError"]},{"text":"impl !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/resolver/struct.Resolver.html\" title=\"struct actix::actors::resolver::Resolver\">Resolver</a>","synthetic":true,"types":["actix::actors::resolver::Resolver"]},{"text":"impl !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/actors/resolver/struct.TcpConnector.html\" title=\"struct actix::actors::resolver::TcpConnector\">TcpConnector</a>","synthetic":true,"types":["actix::actors::resolver::TcpConnector"]},{"text":"impl&lt;Fut, Fn&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/fut/future/enum.Map.html\" title=\"enum actix::fut::future::Map\">Map</a>&lt;Fut, Fn&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Fn: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fut: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::future::map::Map"]},{"text":"impl&lt;A, B, Fn&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"enum\" href=\"actix/fut/future/enum.Then.html\" title=\"enum actix::fut::future::Then\">Then</a>&lt;A, B, Fn&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;B: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fn: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::future::then::Then"]},{"text":"impl&lt;F&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/future/struct.Timeout.html\" title=\"struct actix::fut::future::Timeout\">Timeout</a>&lt;F&gt;","synthetic":true,"types":["actix::fut::future::timeout::Timeout"]},{"text":"impl&lt;F, A&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/future/struct.FutureWrap.html\" title=\"struct actix::fut::future::FutureWrap\">FutureWrap</a>&lt;F, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;F: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::future::FutureWrap"]},{"text":"impl&lt;S, C&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Collect.html\" title=\"struct actix::fut::stream::Collect\">Collect</a>&lt;S, C&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;C: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::collect::Collect"]},{"text":"impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Finish.html\" title=\"struct actix::fut::stream::Finish\">Finish</a>&lt;S&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::finish::Finish"]},{"text":"impl&lt;S, F, Fut, T&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Fold.html\" title=\"struct actix::fut::stream::Fold\">Fold</a>&lt;S, F, Fut, T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;F: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fut: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::fold::Fold"]},{"text":"impl&lt;S, F&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Map.html\" title=\"struct actix::fut::stream::Map\">Map</a>&lt;S, F&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;F: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::map::Map"]},{"text":"impl&lt;S, I, Fn, Fut&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.SkipWhile.html\" title=\"struct actix::fut::stream::SkipWhile\">SkipWhile</a>&lt;S, I, Fn, Fut&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Fn: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fut: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;I: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::skip_while::SkipWhile"]},{"text":"impl&lt;S, I, Fut, Fn&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.TakeWhile.html\" title=\"struct actix::fut::stream::TakeWhile\">TakeWhile</a>&lt;S, I, Fut, Fn&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Fn: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fut: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;I: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::take_while::TakeWhile"]},{"text":"impl&lt;S, Fn, F&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Then.html\" title=\"struct actix::fut::stream::Then\">Then</a>&lt;S, Fn, F&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;F: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;Fn: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::then::Then"]},{"text":"impl&lt;S&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.Timeout.html\" title=\"struct actix::fut::stream::Timeout\">Timeout</a>&lt;S&gt;","synthetic":true,"types":["actix::fut::stream::timeout::Timeout"]},{"text":"impl&lt;S, A&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/fut/stream/struct.StreamWrap.html\" title=\"struct actix::fut::stream::StreamWrap\">StreamWrap</a>&lt;S, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,<br>&nbsp;&nbsp;&nbsp;&nbsp;S: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a>,&nbsp;</span>","synthetic":true,"types":["actix::fut::stream::StreamWrap"]},{"text":"impl&lt;T, E&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/io/struct.Writer.html\" title=\"struct actix::io::Writer\">Writer</a>&lt;T, E&gt;","synthetic":true,"types":["actix::io::Writer"]},{"text":"impl&lt;I, T, U&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/io/struct.FramedWrite.html\" title=\"struct actix::io::FramedWrite\">FramedWrite</a>&lt;I, T, U&gt;","synthetic":true,"types":["actix::io::FramedWrite"]},{"text":"impl&lt;I, S&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/io/struct.SinkWrite.html\" title=\"struct actix::io::SinkWrite\">SinkWrite</a>&lt;I, S&gt;","synthetic":true,"types":["actix::io::SinkWrite"]},{"text":"impl !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/registry/struct.Registry.html\" title=\"struct actix::registry::Registry\">Registry</a>","synthetic":true,"types":["actix::registry::Registry"]},{"text":"impl !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/registry/struct.SystemRegistry.html\" title=\"struct actix::registry::SystemRegistry\">SystemRegistry</a>","synthetic":true,"types":["actix::registry::SystemRegistry"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/sync/struct.SyncArbiter.html\" title=\"struct actix::sync::SyncArbiter\">SyncArbiter</a>&lt;A&gt;","synthetic":true,"types":["actix::sync::SyncArbiter"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/sync/struct.SyncContext.html\" title=\"struct actix::sync::SyncContext\">SyncContext</a>&lt;A&gt;","synthetic":true,"types":["actix::sync::SyncContext"]},{"text":"impl&lt;T&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/utils/struct.Condition.html\" title=\"struct actix::utils::Condition\">Condition</a>&lt;T&gt;","synthetic":true,"types":["actix::utils::Condition"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/utils/struct.TimerFunc.html\" title=\"struct actix::utils::TimerFunc\">TimerFunc</a>&lt;A&gt;","synthetic":true,"types":["actix::utils::TimerFunc"]},{"text":"impl&lt;A&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix/utils/struct.IntervalFunc.html\" title=\"struct actix::utils::IntervalFunc\">IntervalFunc</a>&lt;A&gt;","synthetic":true,"types":["actix::utils::IntervalFunc"]}];
implementors["actix_broker"] = [{"text":"impl&lt;T&gt; !<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix_broker/struct.Broker.html\" title=\"struct actix_broker::Broker\">Broker</a>&lt;T&gt;","synthetic":true,"types":["actix_broker::broker::Broker"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix_broker/struct.SystemBroker.html\" title=\"struct actix_broker::SystemBroker\">SystemBroker</a>","synthetic":true,"types":["actix_broker::broker::SystemBroker"]},{"text":"impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/std/panic/trait.RefUnwindSafe.html\" title=\"trait std::panic::RefUnwindSafe\">RefUnwindSafe</a> for <a class=\"struct\" href=\"actix_broker/struct.ArbiterBroker.html\" title=\"struct actix_broker::ArbiterBroker\">ArbiterBroker</a>","synthetic":true,"types":["actix_broker::broker::ArbiterBroker"]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()