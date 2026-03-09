class DynamicProxy
  def initialize(target)
    @target = target
  end

  def method_missing(name, *args)
    @target.send(name, *args)
  end

  def respond_to_missing?(name, include_private = false)
    true
  end

  def wrap
    find_user()      # → method_missing (unresolvable)
    update_record()  # → method_missing (unresolvable)
    known_helper()   # → resolved normally (Strategy 2)
  end

  def known_helper
    "done"
  end
end
