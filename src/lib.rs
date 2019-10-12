
//! Tries to define and specify the behaviour used in CPC and other solutions
//! where you have one ore more streams delivering data
//! and then 0 or 1 sink that sends data out.

use warp::ws2 as ws;
use std::error::Error as StdError;
use futures::{Sink, Stream, StreamExt, Future, SinkExt};
use std::convert::{TryInto, TryFrom};



pub trait Processor
where Self::ResultFuture: Future<Output = Result<Self::ResultItem, Self::Error>>,
      Self: Sized,
{
    type Item;
    type Error;
    type ResultItem;
    type ResultFuture;

    fn process(&mut self, item: Self::Item) -> Self::ResultFuture;


    fn stopped(self) {}
    fn stopped_with_error(_error: Self::Error) {}

    fn on_error(&mut self, _error: Self::Error) {}

}


pub struct Handle;


struct Runner<TStream, TItem, TError, TSink, TSinkItem, TSinkError>
where TStream: Stream<Item = Result<TItem, TError>> + Unpin + 'static,
      TSink: Sink<TSinkItem, Error = TSinkError> + Unpin + 'static,
{
    stream: TStream,
    sink: TSink,
    _marker: std::marker::PhantomData<TSinkItem>,
}


impl
    <TStream,
     TItem,
     TError,
     TSink,
     TSinkItem,
     TSinkError>
    
    Runner
    <TStream,
     TItem,
     TError,
     TSink,
     TSinkItem,
     TSinkError>

where TStream: Stream<Item = Result<TItem, TError>> + Unpin + 'static,
      TSink: Sink<TSinkItem, Error = TSinkError> + Unpin + 'static,
      TError: From<TSinkError>,
{

    fn new(stream: TStream, sink: TSink) -> Self {
	Self {
	    stream,
	    sink,
	    _marker: Default::default(),
	}
    }

    async fn run<P>(mut self, mut processor: P)
    where P: Processor<Item = TItem, Error = TError, ResultItem = TSinkItem>,
    {
	loop {
	    println!("Polling stream");

	    let incoming = match self.stream.next().await {
		Some(Ok(incoming)) => incoming,
		Some(Err(err)) => {
		    // Add ability for Processor to STOP here by returning a value.
		    processor.on_error(err);
		    continue;
		},

		None => {

		    println!("Stream empty, stopping");
		    processor.stopped();
		    return;
		}
		    
	    };

	    let outgoing = match processor.process(incoming).await {
		Ok(outgoing) => outgoing,
		Err(err) => {
		    processor.on_error(err);
		    continue;
		}
	    };

	    if let Err(_failed) = self.sink.send(outgoing).await {
		// @TODO: Maybe deliver this failed thing to the
		// processor. Allow it to act on the failed item.
		// for now, we just drop it and continue.
		continue;
	    }

	    if let Err(err) = self.sink.flush().await {
		processor.on_error(TError::from(err));
	    }
	}
    }
    

    
}

pub struct Builder<TItem, TError>{
    streams: Vec<Box<dyn Stream<Item = Result<TItem, TError>> + Unpin>>
}



impl
    <TItem,
     TError>

    Builder
    <TItem,
     TError>

where
    TError: 'static,
    TItem: 'static,
{

    pub fn new() -> Self {
	Self { streams: Vec::new() }
    }

    pub fn add_stream<TStream, TStreamItem>(&mut self, stream: TStream) -> &mut Self
    where
	 TStream: Stream<Item = TStreamItem> + Unpin + 'static,
	 TStreamItem: Into<TItem>,
    {
	self.streams.push(Box::new(stream.map(|item| Ok(item.into()))));
	self
    }



    pub async fn run<TSink, TSinkError, TProcessor>(
	&mut self,
	sink: TSink,
	processor: TProcessor
    )
	where
	TProcessor: Processor<Item = TItem, Error = TError>,
	TSink: Sink<TProcessor::ResultItem, Error = TSinkError> + Unpin + 'static,
	TError: From<TSinkError>

    {
	let joined_streams = futures::stream::select_all(self.streams.drain(0..));
	let runner = Runner::new(joined_streams, sink);

	runner.run(processor).await
	
    }

}