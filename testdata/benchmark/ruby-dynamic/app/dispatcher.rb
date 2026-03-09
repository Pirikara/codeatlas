class Dispatcher
  def run
    send(:process)        # static → CALLS edge to process
    public_send(:notify)  # static → CALLS edge to notify
    send(dynamic_method)  # dynamic variable → skip
  end

  def process
    "done"
  end

  def notify
    "notified"
  end
end
